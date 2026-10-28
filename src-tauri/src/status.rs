//! System status watchers: battery, Wi-Fi, Bluetooth, Caps Lock, keyboard
//! language, volume and laptop brightness.
//!
//! Keyboard rule: we read only the Caps Lock toggle state and the active
//! keyboard layout. Typed keys are never read or recorded.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use windows::Devices::Bluetooth::BluetoothDevice;
use windows::Devices::Enumeration::{DeviceInformation, DeviceInformationKind, DeviceInformationUpdate, DeviceWatcher};
use windows::Foundation::{IReference, TypedEventHandler};
use windows::Networking::Connectivity::{NetworkConnectivityLevel, NetworkInformation};
use windows::Win32::Globalization::{
    GetLocaleInfoEx, LCIDToLocaleName, LOCALE_SENGLISHDISPLAYNAME, LOCALE_SISO639LANGNAME,
};
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, GetKeyboardLayout, VK_CAPITAL};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
use windows::core::{GUID, HSTRING, IInspectable, Interface, PCWSTR, implement};

use crate::util::{CmdResult, com_mta, err, sleep_ms};
use crate::{notify, settings};

// ---------------------------------------------------------------------------
// Snapshot of everything, so a freshly opened UI can render immediately.
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Power {
    has_battery: bool,
    charging: bool,
    percent: i32,
}

#[derive(Serialize, Clone, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    connected: bool,
    wifi: bool,
    name: String,
}

#[derive(Serialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    power: Power,
    network: Network,
    caps_lock: bool,
    layout: String,
    volume: f32,
    muted: bool,
}

static SNAP: Mutex<Option<Snapshot>> = Mutex::new(None);

fn update<F: FnOnce(&mut Snapshot)>(f: F) {
    let mut s = SNAP.lock();
    let snap = s.get_or_insert_with(Snapshot::default);
    f(snap);
}

#[tauri::command]
pub fn status_snapshot() -> Snapshot {
    SNAP.lock().clone().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Power
// ---------------------------------------------------------------------------

fn read_power() -> Power {
    let mut s = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut s) }.is_err() {
        return Power::default();
    }
    Power {
        has_battery: s.BatteryFlag != 128 && s.BatteryFlag != 255 && s.BatteryLifePercent != 255,
        charging: s.ACLineStatus == 1,
        percent: if s.BatteryLifePercent == 255 {
            -1
        } else {
            s.BatteryLifePercent as i32
        },
    }
}

struct PowerWatch {
    last: Option<Power>,
    alerted: Vec<i64>,
}

impl PowerWatch {
    fn tick(&mut self) {
        let p = read_power();
        if self.last.as_ref() == Some(&p) {
            return;
        }
        update(|s| s.power = p.clone());
        notify::emit("power", &p);
        if let Some(prev) = &self.last
            && p.has_battery
            && prev.charging != p.charging
        {
            notify::popup("battery", if p.charging { "charging" } else { "unplugged" }, json!(p));
        }
        if p.has_battery && !p.charging && p.percent >= 0 {
            let thresholds: Vec<i64> = settings::get("status.lowBattery")
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_i64()).collect())
                .unwrap_or_else(|| vec![20, 10]);
            // Alert once per threshold per discharge.
            if let Some(t) = thresholds
                .iter()
                .filter(|t| p.percent as i64 <= **t && !self.alerted.contains(t))
                .min()
                .copied()
            {
                for x in thresholds.iter().filter(|x| p.percent as i64 <= **x) {
                    if !self.alerted.contains(x) {
                        self.alerted.push(*x);
                    }
                }
                if settings::get_bool("status.battery", true) {
                    notify::popup("battery", "low", json!({ "percent": p.percent, "threshold": t }));
                    notify::toast(
                        "Battery low",
                        &format!("{}% remaining. Plug in your charger.", p.percent),
                    );
                }
            }
        } else if p.charging {
            self.alerted.clear();
        }
        self.last = Some(p);
    }
}

// ---------------------------------------------------------------------------
// Network
// ---------------------------------------------------------------------------

fn read_network() -> Network {
    let Ok(profile) = NetworkInformation::GetInternetConnectionProfile() else {
        return Network::default();
    };
    let level = profile
        .GetNetworkConnectivityLevel()
        .unwrap_or(NetworkConnectivityLevel::None);
    let connected = level == NetworkConnectivityLevel::InternetAccess
        || level == NetworkConnectivityLevel::ConstrainedInternetAccess
        || level == NetworkConnectivityLevel::LocalAccess;
    let wifi = profile.IsWlanConnectionProfile().unwrap_or(false);
    let name = if wifi {
        profile
            .WlanConnectionProfileDetails()
            .and_then(|d| d.GetConnectedSsid())
            .map(|h| h.to_string())
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| profile.ProfileName().ok().map(|h| h.to_string()))
            .unwrap_or_default()
    } else {
        profile.ProfileName().map(|h| h.to_string()).unwrap_or_default()
    };
    Network { connected, wifi, name }
}

struct NetWatch {
    last: Option<Network>,
}

impl NetWatch {
    fn tick(&mut self) {
        let n = read_network();
        if self.last.as_ref() == Some(&n) {
            return;
        }
        update(|s| s.network = n.clone());
        notify::emit("network", &n);
        if let Some(prev) = &self.last
            && (prev.connected != n.connected || (n.connected && prev.name != n.name))
        {
            let kind = if n.connected { "connected" } else { "disconnected" };
            let data = if n.connected { json!(n) } else { json!(prev) };
            notify::popup("wifi", kind, data);
        }
        self.last = Some(n);
    }
}

// ---------------------------------------------------------------------------
// Caps Lock + keyboard language
// ---------------------------------------------------------------------------

fn locale_info(name: &[u16], kind: u32) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetLocaleInfoEx(PCWSTR(name.as_ptr()), kind, Some(&mut buf)) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..(n as usize).saturating_sub(1)])
}

fn layout_of_foreground() -> Option<(usize, u16, String, String)> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return None;
        }
        let tid = GetWindowThreadProcessId(fg, None);
        let hkl = GetKeyboardLayout(tid);
        let langid = (hkl.0 as usize & 0xFFFF) as u16;
        if langid == 0 {
            return None;
        }
        let mut name = [0u16; 85];
        let len = LCIDToLocaleName(langid as u32, Some(&mut name), 0);
        if len <= 0 {
            return None;
        }
        let code = locale_info(&name, LOCALE_SISO639LANGNAME).to_uppercase();
        let display = locale_info(&name, LOCALE_SENGLISHDISPLAYNAME);
        Some((fg.0 as usize, langid, code, display))
    }
}

struct KeyWatch {
    caps: Option<bool>,
    lang: Option<u16>,
    last_fg: usize,
}

impl KeyWatch {
    fn tick(&mut self) {
        let caps = unsafe { GetKeyState(VK_CAPITAL.0 as i32) & 1 == 1 };
        if self.caps != Some(caps) {
            if self.caps.is_some() {
                notify::popup(
                    "capsLock",
                    if caps { "caps-on" } else { "caps-off" },
                    json!({ "on": caps }),
                );
            }
            update(|s| s.caps_lock = caps);
            notify::emit("caps", json!({ "on": caps }));
            self.caps = Some(caps);
        }
        if let Some((fg, langid, code, display)) = layout_of_foreground() {
            if self.lang != Some(langid) {
                // Switching to a window with its own layout isn't a language change.
                if self.lang.is_some() && fg == self.last_fg {
                    notify::popup("keyboardLang", "language", json!({ "code": code, "name": display }));
                }
                update(|s| s.layout = code.clone());
                self.lang = Some(langid);
            }
            self.last_fg = fg;
        }
    }
}

// ---------------------------------------------------------------------------
// Volume (Core Audio endpoint callback)
// ---------------------------------------------------------------------------

/// Event context for changes we make ourselves (slider), so the HUD skips them.
const OWN_CONTEXT: GUID = GUID::from_u128(0x5e1f0c2a_7c1d_4b8e_9a51_77a3c0d9e001);

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback;

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        if pnotify.is_null() {
            return Ok(());
        }
        let d = unsafe { &*pnotify };
        let level = d.fMasterVolume;
        let muted = d.bMuted.as_bool();
        update(|s| {
            s.volume = level;
            s.muted = muted;
        });
        let own = d.guidEventContext == OWN_CONTEXT;
        notify::emit("volume", json!({ "level": level, "muted": muted, "own": own }));
        Ok(())
    }
}

fn endpoint() -> windows::core::Result<(IAudioEndpointVolume, String)> {
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let id_ptr = dev.GetId()?;
        let id = id_ptr.to_string().unwrap_or_default();
        CoTaskMemFree(Some(id_ptr.0 as *const _));
        let vol: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None)?;
        Ok((vol, id))
    }
}

fn start_volume() {
    crate::util::spawn_loop("volume", || {
        com_mta();
        let cb: IAudioEndpointVolumeCallback = VolumeCallback.into();
        let mut current: Option<(IAudioEndpointVolume, String)> = None;
        loop {
            // Re-register when the default output device changes.
            if let Ok((vol, id)) = endpoint()
                && current.as_ref().map(|(_, c)| c != &id).unwrap_or(true)
            {
                if let Some((old, _)) = current.take() {
                    unsafe {
                        let _ = old.UnregisterControlChangeNotify(&cb);
                    }
                }
                unsafe {
                    let _ = vol.RegisterControlChangeNotify(&cb);
                    let level = vol.GetMasterVolumeLevelScalar().unwrap_or(0.0);
                    let muted = vol.GetMute().map(|b| b.as_bool()).unwrap_or(false);
                    update(|s| {
                        s.volume = level;
                        s.muted = muted;
                    });
                }
                current = Some((vol, id));
            }
            sleep_ms(3000);
        }
    });
}

#[tauri::command]
pub async fn volume_set(level: f32) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        com_mta();
        let (vol, _) = endpoint().map_err(err)?;
        unsafe {
            vol.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), &OWN_CONTEXT)
                .map_err(err)?;
            if level > 0.0 && vol.GetMute().map(|b| b.as_bool()).unwrap_or(false) {
                let _ = vol.SetMute(false, &OWN_CONTEXT);
            }
        }
        Ok(())
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub async fn volume_toggle_mute() -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        com_mta();
        let (vol, _) = endpoint().map_err(err)?;
        unsafe {
            let muted = vol.GetMute().map_err(err)?.as_bool();
            vol.SetMute(!muted, &OWN_CONTEXT).map_err(err)
        }
    })
    .await
    .map_err(err)?
}

// ---------------------------------------------------------------------------
// Brightness (laptop panel only, via WMI)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Debug)]
#[serde(rename = "WmiMonitorBrightnessEvent")]
#[serde(rename_all = "PascalCase")]
struct BrightnessEvent {
    brightness: u8,
}

fn start_brightness() {
    crate::util::spawn_loop("brightness", || {
        com_mta();
        let Ok(con) = wmi::WMIConnection::with_namespace_path("root\\WMI") else {
            return;
        };
        let Ok(events) = con.notification::<BrightnessEvent>() else {
            return;
        };
        for ev in events.flatten() {
            notify::emit("brightness", json!({ "level": ev.brightness }));
        }
    });
}

// ---------------------------------------------------------------------------
// Bluetooth
// ---------------------------------------------------------------------------

const PROP_CONNECTED: &str = "System.Devices.Aep.IsConnected";
const PROP_CONTAINER: &str = "System.Devices.Aep.ContainerId";
/// Battery level exposed by the hands-free / battery service driver.
const PROP_BATTERY: &str = "{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2";

#[derive(Clone, Default)]
struct BtDevice {
    name: String,
    connected: bool,
    container: Option<GUID>,
}

fn prop_bool(map: &windows_collections::IMapView<HSTRING, IInspectable>, key: &str) -> Option<bool> {
    let v = map.Lookup(&HSTRING::from(key)).ok()?;
    v.cast::<IReference<bool>>().ok()?.Value().ok()
}

fn prop_guid(map: &windows_collections::IMapView<HSTRING, IInspectable>, key: &str) -> Option<GUID> {
    let v = map.Lookup(&HSTRING::from(key)).ok()?;
    v.cast::<IReference<GUID>>().ok()?.Value().ok()
}

fn bt_battery(container: GUID) -> Option<u8> {
    let aqs = HSTRING::from(format!("System.Devices.ContainerId:=\"{{{container:?}}}\""));
    let props: windows_collections::IIterable<HSTRING> = vec![HSTRING::from(PROP_BATTERY)].into();
    let list = DeviceInformation::FindAllAsyncWithKindAqsFilterAndAdditionalProperties(
        &aqs,
        &props,
        DeviceInformationKind::Device,
    )
    .ok()?
    .join()
    .ok()?;
    for i in 0..list.Size().ok()? {
        let dev = list.GetAt(i).ok()?;
        let map = dev.Properties().ok()?;
        if let Ok(v) = map.Lookup(&HSTRING::from(PROP_BATTERY))
            && let Ok(b) = v.cast::<IReference<u8>>().and_then(|r| r.Value())
        {
            return Some(b);
        }
    }
    None
}

fn announce(dev: BtDevice) {
    std::thread::spawn(move || {
        com_mta();
        // Battery is published a moment after the connection comes up.
        if dev.connected {
            sleep_ms(2500);
        }
        let battery = if dev.connected {
            dev.container.and_then(bt_battery)
        } else {
            None
        };
        notify::popup(
            "bluetooth",
            if dev.connected {
                "bt-connected"
            } else {
                "bt-disconnected"
            },
            json!({ "name": dev.name, "battery": battery }),
        );
    });
}

fn start_bluetooth() {
    crate::util::spawn_loop("bluetooth", || {
        com_mta();
        let devices: Arc<Mutex<HashMap<String, BtDevice>>> = Arc::new(Mutex::new(HashMap::new()));
        let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let setup = || -> windows::core::Result<DeviceWatcher> {
            let aqs = BluetoothDevice::GetDeviceSelectorFromPairingState(true)?;
            let props: windows_collections::IIterable<HSTRING> =
                vec![HSTRING::from(PROP_CONNECTED), HSTRING::from(PROP_CONTAINER)].into();
            let watcher = DeviceInformation::CreateWatcherWithKindAqsFilterAndAdditionalProperties(
                &aqs,
                &props,
                DeviceInformationKind::AssociationEndpoint,
            )?;
            let d = devices.clone();
            watcher.Added(&TypedEventHandler::<DeviceWatcher, DeviceInformation>::new(
                move |_, info| {
                    if let Some(info) = info.as_ref() {
                        let id = info.Id()?.to_string();
                        let map = info.Properties()?;
                        d.lock().insert(
                            id,
                            BtDevice {
                                name: info.Name()?.to_string(),
                                connected: prop_bool(&map, PROP_CONNECTED).unwrap_or(false),
                                container: prop_guid(&map, PROP_CONTAINER),
                            },
                        );
                    }
                    Ok(())
                },
            ))?;
            let d = devices.clone();
            let r = ready.clone();
            watcher.Updated(&TypedEventHandler::<DeviceWatcher, DeviceInformationUpdate>::new(
                move |_, upd| {
                    if let Some(upd) = upd.as_ref() {
                        let id = upd.Id()?.to_string();
                        let map = upd.Properties()?;
                        if let Some(connected) = prop_bool(&map, PROP_CONNECTED) {
                            let mut guard = d.lock();
                            if let Some(dev) = guard.get_mut(&id)
                                && dev.connected != connected
                            {
                                dev.connected = connected;
                                if r.load(std::sync::atomic::Ordering::Relaxed) {
                                    announce(dev.clone());
                                }
                            }
                        }
                    }
                    Ok(())
                },
            ))?;
            let d = devices.clone();
            watcher.Removed(&TypedEventHandler::<DeviceWatcher, DeviceInformationUpdate>::new(
                move |_, upd| {
                    if let Some(upd) = upd.as_ref() {
                        d.lock().remove(&upd.Id()?.to_string());
                    }
                    Ok(())
                },
            ))?;
            let r = ready.clone();
            watcher.EnumerationCompleted(&TypedEventHandler::<DeviceWatcher, IInspectable>::new(move |_, _| {
                r.store(true, std::sync::atomic::Ordering::Relaxed);
                Ok(())
            }))?;
            watcher.Start()?;
            Ok(watcher)
        };
        // Keep the watcher alive for the life of the app.
        let _watcher = setup();
        loop {
            sleep_ms(60_000);
        }
    });
}

// ---------------------------------------------------------------------------

pub fn start() {
    start_volume();
    start_brightness();
    start_bluetooth();
    crate::util::spawn_loop("status", || {
        com_mta();
        let mut power = PowerWatch {
            last: None,
            alerted: Vec::new(),
        };
        let mut net = NetWatch { last: None };
        let mut keys = KeyWatch {
            caps: None,
            lang: None,
            last_fg: 0,
        };
        let mut tick: u64 = 0;
        loop {
            keys.tick();
            if tick.is_multiple_of(10) {
                power.tick();
            }
            if tick.is_multiple_of(15) {
                net.tick();
            }
            tick = tick.wrapping_add(1);
            sleep_ms(200);
        }
    });
}
