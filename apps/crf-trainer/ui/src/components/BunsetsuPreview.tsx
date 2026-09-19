import type { Bunsetsu } from "../types";

interface BunsetsuPreviewProps {
  bunsetsu: Bunsetsu[];
  /** `large` is used for the headline result, `inline` inside sample lists. */
  size?: "normal" | "large" | "inline";
  emptyText?: string;
}

/**
 * Render a bunsetsu split the way the GTK panel did with Pango markup:
 * lookup bunsetsu bold, passthrough bunsetsu in normal weight.
 *
 * GTKパネルのPangoマークアップと同じ表示: ルックアップ文節は太字、
 * パススルー文節は通常ウェイト。
 */
export default function BunsetsuPreview({
  bunsetsu,
  size = "normal",
  emptyText = "(no result)",
}: BunsetsuPreviewProps) {
  if (!bunsetsu.length) {
    return <span className="empty-note">{emptyText}</span>;
  }

  return (
    <div className={`bunsetsu-preview size-${size}`}>
      {bunsetsu.map((item, index) => (
        <span
          key={`${item.text}-${index}`}
          className={item.isLookup ? "bunsetsu lookup" : "bunsetsu passthrough"}
          title={item.isLookup ? "Lookup (dictionary conversion)" : "Passthrough (output as-is)"}
        >
          {item.text}
        </span>
      ))}
    </div>
  );
}
