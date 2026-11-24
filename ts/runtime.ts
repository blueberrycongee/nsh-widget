import type { JSX } from "nsh";
import { Widget as NativeWidget } from "nsh-widget/native";
import {
  type RawWidgetSnapshot,
  type RawWidgetTimelineEntry,
  type RawWidgetTree,
  toRawWidgetNode,
  type WidgetElement,
  type WidgetFamily,
  type WidgetReloadPolicy,
} from "./dsl";

export interface WidgetColor {
  light: [number, number, number, number];
  dark: [number, number, number, number];
}

export type WidgetFill =
  | { kind: "solid"; color: WidgetColor }
  | {
      kind: "linearGradient";
      angleDeg: number;
      stops: Array<{ color: WidgetColor; position: number }>;
    };

export interface WidgetEdges {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

export interface WidgetLayout {
  axis: "row" | "column" | "stack";
  mainAlign: "start" | "center" | "end" | "spaceBetween" | "spaceAround" | "spaceEvenly";
  crossAlign: "start" | "center" | "end" | "stretch";
  gap: number;
  padding: WidgetEdges;
  width?: number;
  height?: number;
  minWidth?: number;
  minHeight?: number;
  maxWidth?: number;
  maxHeight?: number;
  flexGrow: number;
}

export interface WidgetBoxStyle {
  background?: WidgetFill;
  borderColor?: WidgetColor;
  borderWidth: number;
  cornerRadius: number;
  opacity: number;
  clip: boolean;
  accentable: boolean;
}

export interface WidgetTextStyle {
  fontSize: number;
  fontWeight: number;
  italic: boolean;
  monospaced: boolean;
  color?: WidgetColor;
  underline: boolean;
  strikethrough: boolean;
  letterSpacing?: number;
}

export interface WidgetTextRun {
  text: string;
  style: WidgetTextStyle;
}

export type WidgetImageSource =
  | { kind: "asset"; path: string }
  | { kind: "file"; path: string }
  | { kind: "symbol"; name: string };

export type WidgetIrNode =
  | { kind: "box"; layout: WidgetLayout; style: WidgetBoxStyle; children: WidgetIrNode[] }
  | {
      kind: "text";
      runs: WidgetTextRun[];
      align: "leading" | "center" | "trailing";
      lineLimit?: number;
      accentable: boolean;
      layout: WidgetLayout;
    }
  | {
      kind: "date";
      timestampMs: number;
      mode: "relative" | "timer" | "offset" | "time" | "date";
      style: WidgetTextStyle;
      accentable: boolean;
      layout: WidgetLayout;
    }
  | {
      kind: "image";
      source: WidgetImageSource;
      resizeMode: "contain" | "cover" | "stretch" | "center";
      tint?: WidgetColor;
      accentedMode: "fullColor" | "accented" | "desaturated" | "template";
      style: WidgetBoxStyle;
      layout: WidgetLayout;
    }
  | {
      kind: "progress";
      value: number;
      trackColor?: WidgetColor;
      indicatorColor?: WidgetColor;
      cornerRadius?: number;
      layout: WidgetLayout;
    }
  | { kind: "link"; href: string; accessibilityLabel?: string; child: WidgetIrNode }
  | {
      kind: "action";
      action: string;
      payloadJson?: string;
      patchJson?: string;
      accessibilityLabel?: string;
      child: WidgetIrNode;
    };

export interface WidgetFamilyTree {
  url?: string;
  containerBackground?: WidgetFill;
  root: WidgetIrNode;
}

export interface WidgetTimelineIrEntry {
  dateMs: number;
  relevance?: number;
  families: Partial<Record<WidgetFamily, WidgetFamilyTree>>;
}

export interface WidgetSnapshot {
  version: number;
  widgetId: string;
  generatedAtMs: number;
  reloadAfterMs?: number;
  reloadNever?: boolean;
  entries: WidgetTimelineIrEntry[];
}

export interface WidgetActionEvent {
  widgetId: string;
  action: string;
  payload?: unknown;
  timestamp: number;
}

export interface WidgetRenderContext<TData> {
  data: TData;
  family: WidgetFamily;
  isPlaceholder: boolean;
}

export interface WidgetDefinition<TData> {
  /** Widget id matching `nsh-widget.json`. */
  id: string;
  /** Display name (used by `WidgetPreview`). */
  name?: string;
  /** Supported size families. */
  families: WidgetFamily[];
  /** Default placeholder payload. */
  placeholder: TData;
  /** Optional root deep link URL (`SwiftUI .widgetURL`). */
  url?: (ctx: { data: TData }) => string | undefined;
  /** Declarative widget render function evaluated across all `families`. */
  render: (ctx: WidgetRenderContext<TData>) => WidgetElement | JSX.Element;
}

export interface WidgetTimelineInput<TData> {
  date: number | Date;
  data: TData;
  relevance?: number;
}

export interface WidgetUpdateOptions {
  reloadPolicy?: WidgetReloadPolicy;
}

export interface WidgetConfigureOptions {
  appGroup?: string;
  themeJson?: string;
  assetsDir?: string;
}

/** Defines a strongly typed WidgetKit widget template. */
export function defineWidget<TData>(def: WidgetDefinition<TData>): WidgetDefinition<TData> {
  return def;
}

function buildRawTree<TData>(
  def: WidgetDefinition<TData>,
  data: TData,
  family: WidgetFamily,
  isPlaceholder: boolean,
): RawWidgetTree {
  const el = def.render({ data, family, isPlaceholder });
  return {
    url: def.url ? def.url({ data }) : undefined,
    root: toRawWidgetNode(el),
  };
}

function buildRawSnapshot<TData>(
  def: WidgetDefinition<TData>,
  entries: Array<{ dateMs: number; data: TData; relevance?: number; isPlaceholder: boolean }>,
  options?: WidgetUpdateOptions,
): RawWidgetSnapshot {
  const rawEntries: RawWidgetTimelineEntry[] = entries.map((entry) => {
    const families: Partial<Record<WidgetFamily, RawWidgetTree>> = {};
    for (const family of def.families) {
      families[family] = buildRawTree(def, entry.data, family, entry.isPlaceholder);
    }
    return {
      dateMs: entry.dateMs,
      relevance: entry.relevance,
      families,
    };
  });
  return {
    widgetId: def.id,
    generatedAtMs: Date.now(),
    reloadPolicy: options?.reloadPolicy,
    entries: rawEntries,
  };
}

/** High-level WidgetKit API combining TSX compilation and native App Group sync. */
export const Widget = {
  /** Returns `true` when macOS WidgetKit App Group sharing is active. */
  available(): Promise<boolean> {
    return NativeWidget.available();
  },

  /** Optionally sets explicit `appGroup`, `themeJson`, or `assetsDir`. */
  configure(options: WidgetConfigureOptions): void {
    NativeWidget.configure(options);
  },

  /** Returns the resolved App Group identifier, if any. */
  appGroup(): string | null {
    return NativeWidget.appGroup();
  },

  /** Compiles a `WidgetDefinition` synchronously into dual-appearance IR. */
  compile<TData>(
    def: WidgetDefinition<TData>,
    data?: TData,
    options?: WidgetUpdateOptions & { isPlaceholder?: boolean },
  ): WidgetSnapshot {
    const resolved = data ?? def.placeholder;
    const raw = buildRawSnapshot(
      def,
      [
        {
          dateMs: Date.now(),
          data: resolved,
          isPlaceholder: options?.isPlaceholder ?? data === undefined,
        },
      ],
      options,
    );
    return NativeWidget.compileSnapshot(raw) as WidgetSnapshot;
  },

  /** Evaluates `def`, persists `data`, writes `snapshot.json`, and reloads WidgetKit. */
  async update<TData>(
    def: WidgetDefinition<TData>,
    data: TData,
    options?: WidgetUpdateOptions,
  ): Promise<void> {
    const raw = buildRawSnapshot(
      def,
      [{ dateMs: Date.now(), data, isPlaceholder: false }],
      options,
    );
    await NativeWidget.setData(def.id, data);
    await NativeWidget.pushSnapshot(raw);
  },

  /** Evaluates a multi-entry timeline and schedules it with WidgetKit. */
  async setTimeline<TData>(
    def: WidgetDefinition<TData>,
    entries: WidgetTimelineInput<TData>[],
    options?: WidgetUpdateOptions,
  ): Promise<void> {
    if (entries.length === 0) {
      throw new RangeError("Widget.setTimeline: entries must not be empty");
    }
    const normalized = entries.map((e) => ({
      dateMs: Math.round(e.date instanceof Date ? e.date.getTime() : e.date),
      data: e.data,
      relevance: e.relevance,
      isPlaceholder: false,
    }));
    const raw = buildRawSnapshot(def, normalized, options);
    await NativeWidget.setData(def.id, normalized[0].data);
    await NativeWidget.pushSnapshot(raw);
  },

  /** Writes a pre-compiled `WidgetSnapshot` IR object directly. */
  writeSnapshot(snapshot: WidgetSnapshot): Promise<void> {
    return NativeWidget.writeRawSnapshot(snapshot);
  },

  /** Persists raw JSON `data` for `widgetId`. */
  setData(widgetId: string, data: unknown): Promise<void> {
    return NativeWidget.setData(widgetId, data);
  },

  /** Reads the last persisted data for `widgetId`. */
  getData<TData = unknown>(widgetId: string): Promise<TData | null> {
    return NativeWidget.getData(widgetId) as Promise<TData | null>;
  },

  /** Requests `WidgetCenter` to reload `widgetId`, or all widgets if omitted. */
  reload(widgetId?: string): Promise<void> {
    return NativeWidget.reload(widgetId);
  },

  /** Subscribes to interactive `<WAction>` button events from desktop widgets. */
  on(event: "action", listener: (event: WidgetActionEvent) => void): () => void {
    return NativeWidget.on(event, listener);
  },
};
