export interface Message {
  readonly id: string;
  readonly text: string;
  readonly createdAt: number;
  readonly senderId: string;
  readonly recipientId: string | null;
  readonly direction: "local" | "outgoing" | "incoming";
  readonly receivedAt: number | null;
  readonly deliveryStatus?: "pending" | "acknowledged" | null;
  readonly acknowledgedAt?: number | null;
}

export function messageLocalTime(message: Message): number {
  return message.receivedAt ?? message.createdAt;
}
