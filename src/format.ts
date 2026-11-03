const usdSmall = new Intl.NumberFormat("pt-BR", { style: "currency", currency: "USD", minimumFractionDigits: 4, maximumFractionDigits: 4 });
const usdLarge = new Intl.NumberFormat("pt-BR", { style: "currency", currency: "USD" });

// Centavos de dólar somem com duas casas; abaixo de US$ 0,01 mostra quatro.
export function usd(value: number): string {
  if (value === 0) return usdLarge.format(0);
  return value < 0.01 ? usdSmall.format(value) : usdLarge.format(value);
}

export function count(value: number): string {
  return value.toLocaleString("pt-BR");
}

export function bytes(value: number): string {
  const gb = value / 1024 ** 3;
  return gb >= 1 ? `${gb.toLocaleString("pt-BR", { maximumFractionDigits: 1 })} GB` : `${Math.round(value / 1024 ** 2)} MB`;
}

export function duration(seconds: number): string {
  if (seconds < 60) return "agora há pouco";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min`;
  return `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}

// Datas do SQLite vêm em UTC ("AAAA-MM-DD HH:MM:SS").
export function sqliteDate(utc: string): string {
  const date = new Date(utc.replace(" ", "T") + "Z");
  const sameDay = date.toDateString() === new Date().toDateString();
  return sameDay
    ? date.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })
    : date.toLocaleDateString("pt-BR", { day: "2-digit", month: "short" });
}
