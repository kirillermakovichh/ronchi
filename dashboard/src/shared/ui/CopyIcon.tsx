export function CopyIcon(props: { copied: boolean }) {
  return (
    <span
      class="copy-glyph"
      classList={{ "is-copied": props.copied }}
      aria-hidden="true"
    />
  );
}
