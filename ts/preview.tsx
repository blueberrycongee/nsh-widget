import { signal, Text, View, type JSX, type TextStyle, type ViewStyle } from "nsh";
import { Button, Progress } from "nsh/base";
import type { WidgetFamily } from "./dsl";
import {
  Widget,
  type WidgetColor,
  type WidgetDefinition,
  type WidgetFamilyTree,
  type WidgetFill,
  type WidgetIrNode,
  type WidgetTextStyle,
} from "./runtime";

export interface WidgetPreviewProps<TData> {
  widget: WidgetDefinition<TData>;
  data?: TData;
  onAction?: (event: { widgetId: string; action: string; payload?: unknown }) => void;
  onOpenUrl?: (url: string) => void;
}

const SIZES: Record<WidgetFamily, { w: number; h: number; label: string }> = {
  small: { w: 170, h: 170, label: "Small (170×170)" },
  medium: { w: 364, h: 170, label: "Medium (364×170)" },
  large: { w: 364, h: 382, label: "Large (364×382)" },
};

function rgbaString(rgba: [number, number, number, number]): string {
  const r = Math.round((rgba[0] ?? 0) * 255);
  const g = Math.round((rgba[1] ?? 0) * 255);
  const b = Math.round((rgba[2] ?? 0) * 255);
  const a = rgba[3] ?? 1;
  return `rgba(${r}, ${g}, ${b}, ${a.toFixed(3)})`;
}

function pickColor(
  color: WidgetColor | undefined,
  dark: boolean,
  tinted: boolean,
): string | undefined {
  if (!color) return undefined;
  if (tinted) {
    // Keep neutral text legible, tint chromatic highlights.
    const c = dark ? color.dark : color.light;
    const max = Math.max(c[0], c[1], c[2]);
    const min = Math.min(c[0], c[1], c[2]);
    if (c[3] > 0.05 && max - min > 0.14) {
      return dark ? "rgba(255, 255, 255, 0.92)" : "rgba(24, 24, 27, 0.92)";
    }
  }
  return rgbaString(dark ? color.dark : color.light);
}

function fillBackground(fill: WidgetFill | undefined, dark: boolean): string | undefined {
  if (!fill) return undefined;
  if (fill.kind === "solid") {
    return rgbaString(dark ? fill.color.dark : fill.color.light);
  }
  const first = fill.stops[0];
  return first ? rgbaString(dark ? first.color.dark : first.color.light) : undefined;
}

function mapMainAlign(
  align: string,
): ViewStyle["justifyContent"] {
  switch (align) {
    case "center":
      return "center";
    case "end":
      return "flex-end";
    case "spaceBetween":
      return "space-between";
    case "spaceAround":
      return "space-around";
    case "spaceEvenly":
      return "space-evenly";
    default:
      return "flex-start";
  }
}

function mapCrossAlign(
  align: string,
): ViewStyle["alignItems"] {
  switch (align) {
    case "center":
      return "center";
    case "end":
      return "flex-end";
    case "stretch":
      return "stretch";
    default:
      return "flex-start";
  }
}

function formatDatePreview(timestampMs: number, mode: string): string {
  const d = new Date(timestampMs);
  if (mode === "time") return d.toLocaleTimeString();
  if (mode === "date") return d.toLocaleDateString();
  const diffSec = Math.round((Date.now() - timestampMs) / 1000);
  if (mode === "timer" || mode === "offset") {
    const abs = Math.abs(diffSec);
    const m = Math.floor(abs / 60);
    const s = abs % 60;
    return `${diffSec >= 0 ? "" : "-"}${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  if (Math.abs(diffSec) < 60) return "just now";
  const mins = Math.round(Math.abs(diffSec) / 60);
  return diffSec >= 0 ? `${mins}m ago` : `in ${mins}m`;
}

function textStyleProps(style: WidgetTextStyle, dark: boolean, tinted: boolean): TextStyle {
  return {
    fontSize: style.fontSize,
    fontWeight: String(style.fontWeight) as TextStyle["fontWeight"],
    fontStyle: style.italic ? "italic" : "normal",
    fontFamily: style.monospaced ? "monospace" : undefined,
    color: pickColor(style.color, dark, tinted),
    letterSpacing: style.letterSpacing,
  };
}

/**
 * Pure-TSX local WidgetKit preview component for NSH apps.
 *
 * Compiles the widget through the exact Rust dual-appearance Tailwind compiler
 * and renders Light / Dark / Tinted cards at macOS Small / Medium / Large
 * dimensions with interactive Action and Link simulation.
 */
export function WidgetPreview<TData>(props: WidgetPreviewProps<TData>) {
  const appearance = signal<"light" | "dark">("light");
  const tinted = signal(false);

  const renderNode = (
    node: WidgetIrNode,
    dark: boolean,
    isTinted: boolean,
  ): JSX.Element => {
    if (node.kind === "box") {
      const l = node.layout;
      const boxStyle: ViewStyle = {
        flexDirection: l.axis === "row" ? "row" : "column",
        justifyContent: mapMainAlign(l.mainAlign),
        alignItems: mapCrossAlign(l.crossAlign),
        gap: l.gap || undefined,
        paddingTop: l.padding.top || undefined,
        paddingRight: l.padding.right || undefined,
        paddingBottom: l.padding.bottom || undefined,
        paddingLeft: l.padding.left || undefined,
        width: l.width,
        height: l.height,
        minWidth: l.minWidth,
        minHeight: l.minHeight,
        maxWidth: l.maxWidth,
        maxHeight: l.maxHeight,
        flexGrow: l.flexGrow > 0 ? l.flexGrow : undefined,
        backgroundColor: fillBackground(node.style.background, dark),
        borderColor: pickColor(node.style.borderColor, dark, isTinted),
        borderWidth: node.style.borderWidth || undefined,
        borderRadius: node.style.cornerRadius || undefined,
        opacity: node.style.opacity < 1 ? node.style.opacity : undefined,
      };
      return (
        <View style={boxStyle}>
          {node.children.map((c) => renderNode(c, dark, isTinted))}
        </View>
      );
    }

    if (node.kind === "text") {
      const first = node.runs[0];
      const combined = node.runs.map((r) => r.text).join("");
      const style: TextStyle = {
        ...(first ? textStyleProps(first.style, dark, isTinted) : {}),
        numberOfLines: node.lineLimit,
        textAlign:
          node.align === "center"
            ? "center"
            : node.align === "trailing"
              ? "right"
              : "left",
      };
      return <Text style={style}>{combined}</Text>;
    }

    if (node.kind === "date") {
      return (
        <Text style={textStyleProps(node.style, dark, isTinted)}>
          {formatDatePreview(node.timestampMs, node.mode)}
        </Text>
      );
    }

    if (node.kind === "progress") {
      return (
        <Progress
          value={Math.round(node.value * 100)}
          trackStyle={{
            height: node.layout.height ?? 6,
            borderRadius: node.cornerRadius ?? 999,
            backgroundColor: pickColor(node.trackColor, dark, isTinted) ?? "#e5e7eb",
          }}
          indicatorStyle={{
            height: node.layout.height ?? 6,
            borderRadius: node.cornerRadius ?? 999,
            backgroundColor: pickColor(node.indicatorColor, dark, isTinted) ?? "#18181b",
          }}
        />
      );
    }

    if (node.kind === "image") {
      const label =
        node.source.kind === "symbol"
          ? `[${node.source.name}]`
          : `[img:${node.source.path}]`;
      return (
        <Text
          style={{
            fontSize: 11,
            color: pickColor(node.tint, dark, isTinted) ?? (dark ? "#a1a1aa" : "#52525b"),
          }}
        >
          {label}
        </Text>
      );
    }

    if (node.kind === "link") {
      return (
        <Button onPress={() => props.onOpenUrl?.(node.href)}>
          {renderNode(node.child, dark, isTinted)}
        </Button>
      );
    }

    if (node.kind === "action") {
      const payload = node.payloadJson ? JSON.parse(node.payloadJson) : undefined;
      return (
        <Button
          onPress={() =>
            props.onAction?.({
              widgetId: props.widget.id,
              action: node.action,
              payload,
            })
          }
        >
          {renderNode(node.child, dark, isTinted)}
        </Button>
      );
    }

    return null;
  };

  const renderCard = (family: WidgetFamily, tree: WidgetFamilyTree | undefined) => {
    const dim = SIZES[family];
    const dark = appearance() === "dark";
    const isTinted = tinted();
    const bg =
      fillBackground(tree?.containerBackground, dark) ?? (dark ? "#18181b" : "#ffffff");

    return (
      <View class="flex-col gap-1.5">
        <Text class="text-xs font-medium text-muted-foreground">{dim.label}</Text>
        <View
          style={{
            width: dim.w,
            height: dim.h,
            borderRadius: 20,
            overflow: "hidden",
            backgroundColor: bg,
            borderWidth: 1,
            borderColor: dark ? "rgba(255,255,255,0.12)" : "rgba(0,0,0,0.08)",
          }}
        >
          {tree ? renderNode(tree.root, dark, isTinted) : null}
        </View>
      </View>
    );
  };

  return (
    <View class="flex-col gap-3 p-4 rounded-xl bg-card border border-border">
      <View class="flex-row items-center justify-between">
        <Text class="text-sm font-semibold text-foreground">
          {`Widget Preview · ${props.widget.name ?? props.widget.id}`}
        </Text>
        <View class="flex-row gap-2">
          <Button
            class={
              appearance() === "light"
                ? "px-2.5 py-1 rounded-md bg-primary"
                : "px-2.5 py-1 rounded-md bg-secondary border border-border"
            }
            onPress={() => appearance.set("light")}
          >
            <Text
              class={
                appearance() === "light"
                  ? "text-xs font-medium text-primary-foreground"
                  : "text-xs font-medium text-secondary-foreground"
              }
            >
              Light
            </Text>
          </Button>
          <Button
            class={
              appearance() === "dark"
                ? "px-2.5 py-1 rounded-md bg-primary"
                : "px-2.5 py-1 rounded-md bg-secondary border border-border"
            }
            onPress={() => appearance.set("dark")}
          >
            <Text
              class={
                appearance() === "dark"
                  ? "text-xs font-medium text-primary-foreground"
                  : "text-xs font-medium text-secondary-foreground"
              }
            >
              Dark
            </Text>
          </Button>
          <Button
            class={
              tinted()
                ? "px-2.5 py-1 rounded-md bg-primary"
                : "px-2.5 py-1 rounded-md bg-secondary border border-border"
            }
            onPress={() => tinted.set(!tinted())}
          >
            <Text
              class={
                tinted()
                  ? "text-xs font-medium text-primary-foreground"
                  : "text-xs font-medium text-secondary-foreground"
              }
            >
              Tinted
            </Text>
          </Button>
        </View>
      </View>

      <View class="flex-row flex-wrap gap-4">
        {() => {
          const snapshot = Widget.compile(props.widget, props.data);
          const entry = snapshot.entries[0];
          return props.widget.families.map((family) =>
            renderCard(family, entry?.families[family]),
          );
        }}
      </View>
    </View>
  );
}
