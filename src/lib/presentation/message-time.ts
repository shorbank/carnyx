const formatter = new Intl.DateTimeFormat("de-AT", {
  hour: "2-digit", minute: "2-digit", hourCycle: "h23",
});

export function formatMessageTime(createdAt: number): string {
  return formatter.format(createdAt);
}
