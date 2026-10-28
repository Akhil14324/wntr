/** WMO weather codes used by Open-Meteo. */
export function weatherInfo(code: number | undefined, isDay = true): { label: string; icon: string } {
  switch (code) {
    case 0:
      return { label: "Clear", icon: isDay ? "sun" : "moon" };
    case 1:
      return { label: "Mostly clear", icon: isDay ? "sun" : "moon" };
    case 2:
      return { label: "Partly cloudy", icon: isDay ? "cloud-sun" : "cloud-moon" };
    case 3:
      return { label: "Overcast", icon: "cloud" };
    case 45:
    case 48:
      return { label: "Fog", icon: "fog" };
    case 51:
    case 53:
    case 55:
      return { label: "Drizzle", icon: "drizzle" };
    case 56:
    case 57:
      return { label: "Freezing drizzle", icon: "drizzle" };
    case 61:
    case 63:
      return { label: "Rain", icon: "rain" };
    case 65:
      return { label: "Heavy rain", icon: "rain" };
    case 66:
    case 67:
      return { label: "Freezing rain", icon: "rain" };
    case 71:
    case 73:
    case 75:
    case 77:
      return { label: "Snow", icon: "snow" };
    case 80:
    case 81:
    case 82:
      return { label: "Showers", icon: "rain" };
    case 85:
    case 86:
      return { label: "Snow showers", icon: "snow" };
    case 95:
      return { label: "Thunderstorm", icon: "storm" };
    case 96:
    case 99:
      return { label: "Storm with hail", icon: "storm" };
    default:
      return { label: "—", icon: "cloud" };
  }
}
