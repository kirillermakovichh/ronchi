interface ViewState {
  expanded: string[];
  technical: boolean;
  scroll: number;
}
const views = new Map<string, ViewState>();
export function readView(key: string): ViewState | undefined {
  const value = views.get(key);
  return value ? { ...value, expanded: [...value.expanded] } : undefined;
}
export function saveView(key: string, value: ViewState) {
  views.delete(key);
  views.set(key, { ...value, expanded: [...value.expanded] });
  while (views.size > 16) views.delete(views.keys().next().value!);
}
