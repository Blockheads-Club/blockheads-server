export interface ToastItem {
  id: number;
  message: string;
  actionLabel?: string;
  action?: () => void;
}

let seq = 0;
export const toasts = $state<{ items: ToastItem[] }>({ items: [] });

/** Show a short confirmation; it disappears after 6 seconds. */
export function toast(message: string, action?: { label: string; run: () => void }, ms = 6000) {
  const id = ++seq;
  toasts.items = [...toasts.items, { id, message, actionLabel: action?.label, action: action?.run }];
  setTimeout(() => dismiss(id), ms);
  return id;
}

export function dismiss(id: number) {
  toasts.items = toasts.items.filter((x) => x.id !== id);
}
