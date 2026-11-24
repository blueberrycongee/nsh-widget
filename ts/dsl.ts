import type { JSX } from "nsh";

export type WidgetFamily = "small" | "medium" | "large";

export type WidgetDateMode = "relative" | "timer" | "offset" | "time" | "date";

export type WidgetResizeMode = "contain" | "cover" | "stretch" | "center";

export type WidgetAccentedImageMode = "fullColor" | "accented" | "desaturated" | "template";

export interface WidgetHints {
  accentable?: boolean;
  accentedRenderingMode?: WidgetAccentedImageMode;
  containerBackground?: boolean;
}

export interface WidgetStyle {
  flexDirection?: "row" | "row-reverse" | "column" | "column-reverse";
  justifyContent?:
    | "flex-start"
    | "start"
    | "center"
    | "flex-end"
    | "end"
    | "space-between"
    | "space-around"
    | "space-evenly";
  alignItems?: "flex-start" | "start" | "center" | "flex-end" | "end" | "stretch";
  gap?: number;
  padding?: number;
  paddingHorizontal?: number;
  paddingVertical?: number;
  paddingTop?: number;
  paddingRight?: number;
  paddingBottom?: number;
  paddingLeft?: number;
  width?: number;
  height?: number;
  minWidth?: number;
  minHeight?: number;
  maxWidth?: number;
  maxHeight?: number;
  flexGrow?: number;
  backgroundColor?: string;
  borderColor?: string;
  borderWidth?: number;
  borderRadius?: number;
  opacity?: number;
  overflow?: "visible" | "hidden";
  color?: string;
  fontSize?: number;
  fontWeight?: number;
  fontStyle?: "normal" | "italic";
  fontFamily?: string;
  letterSpacing?: number;
  textAlign?: "auto" | "left" | "center" | "right";
  tintColor?: string;
}

export interface RawWidgetTextRun {
  text: string;
  class?: string;
  style?: WidgetStyle;
}

export type RawWidgetNode =
  | {
      kind: "box" | "row" | "col" | "stack";
      axis?: "row" | "column" | "stack";
      class?: string;
      style?: WidgetStyle;
      widget?: WidgetHints;
      children: RawWidgetNode[];
    }
  | {
      kind: "text";
      text?: string;
      runs?: RawWidgetTextRun[];
      class?: string;
      style?: WidgetStyle;
      numberOfLines?: number;
      widget?: WidgetHints;
    }
  | {
      kind: "date";
      dateMs: number;
      mode?: WidgetDateMode;
      class?: string;
      style?: WidgetStyle;
      widget?: WidgetHints;
    }
  | {
      kind: "image";
      source: string;
      resizeMode?: WidgetResizeMode;
      class?: string;
      style?: WidgetStyle;
      widget?: WidgetHints;
    }
  | {
      kind: "progress";
      value: number;
      max?: number;
      class?: string;
      style?: WidgetStyle;
      trackClass?: string;
      trackStyle?: WidgetStyle;
      indicatorClass?: string;
      indicatorStyle?: WidgetStyle;
    }
  | {
      kind: "link";
      href: string;
      accessibilityLabel?: string;
      class?: string;
      style?: WidgetStyle;
      children: RawWidgetNode[];
    }
  | {
      kind: "action";
      action: string;
      payload?: unknown;
      patch?: Record<string, unknown>;
      accessibilityLabel?: string;
      class?: string;
      style?: WidgetStyle;
      children: RawWidgetNode[];
    };

export interface RawWidgetTree {
  url?: string;
  root: RawWidgetNode;
}

export interface RawWidgetTimelineEntry {
  dateMs: number;
  relevance?: number;
  families: Partial<Record<WidgetFamily, RawWidgetTree>>;
}

export type WidgetReloadPolicy = "atEnd" | "never" | { after: number };

export interface RawWidgetSnapshot {
  widgetId: string;
  generatedAtMs?: number;
  reloadPolicy?: WidgetReloadPolicy;
  entries: RawWidgetTimelineEntry[];
}

/**
 * Callable wrapper assignable to NSH's `JSX.Element` (`() => null`), carrying
 * the declarative widget node for the Rust compiler.
 */
export type WidgetElement = (() => null) & {
  readonly __nshWidgetNode: RawWidgetNode;
};

export type WidgetChild =
  | JSX.Element
  | WidgetElement
  | string
  | number
  | boolean
  | null
  | undefined
  | (() => WidgetChild)
  | WidgetChild[];

function makeElement(node: RawWidgetNode): WidgetElement {
  const fn = (() => null) as WidgetElement;
  Object.defineProperty(fn, "__nshWidgetNode", {
    value: node,
    enumerable: false,
    configurable: false,
  });
  return fn;
}

function isWidgetElement(value: unknown): value is WidgetElement {
  return typeof value === "function" && "__nshWidgetNode" in value;
}

export function toRawWidgetNode(el: WidgetElement | JSX.Element): RawWidgetNode {
  let cur: unknown = el;
  while (typeof cur === "function" && !isWidgetElement(cur)) {
    cur = (cur as () => unknown)();
  }
  if (isWidgetElement(cur)) {
    return cur.__nshWidgetNode;
  }
  throw new TypeError(
    "Widget render() must return a nsh-widget element (<WBox>, <WCol>, <WRow>, <WStack>, <WText>, ...).",
  );
}

function flattenChildren(input: WidgetChild, out: RawWidgetNode[] = []): RawWidgetNode[] {
  if (input === null || input === undefined || typeof input === "boolean") {
    return out;
  }
  if (Array.isArray(input)) {
    for (const item of input) {
      flattenChildren(item, out);
    }
    return out;
  }
  if (isWidgetElement(input)) {
    out.push(input.__nshWidgetNode);
    return out;
  }
  if (typeof input === "function") {
    // `nsh-bundler` wraps dynamic expressions among multiple JSX siblings in `() => expr`.
    return flattenChildren((input as () => WidgetChild)(), out);
  }
  if (typeof input === "string" || typeof input === "number") {
    out.push({ kind: "text", text: String(input) });
  }
  return out;
}

function flattenTextRuns(
  input: WidgetChild,
): { text?: string; runs?: RawWidgetTextRun[] } {
  const nodes = flattenChildren(input);
  if (nodes.length === 0) {
    return { text: "" };
  }
  const hasStyledSpan = nodes.some(
    (n) => n.kind === "text" && (Boolean(n.class) || Boolean(n.style) || (n.runs && n.runs.length > 0)),
  );
  if (!hasStyledSpan && nodes.every((n) => n.kind === "text")) {
    return {
      text: nodes.map((n) => (n.kind === "text" ? (n.text ?? "") : "")).join(""),
    };
  }
  const runs: RawWidgetTextRun[] = [];
  for (const node of nodes) {
    if (node.kind !== "text") continue;
    if (node.runs && node.runs.length > 0) {
      for (const r of node.runs) {
        runs.push(r);
      }
    } else if (node.text !== undefined) {
      runs.push({
        text: node.text,
        class: node.class,
        style: node.style,
      });
    }
  }
  return { runs };
}

export interface WBoxProps {
  axis?: "row" | "column" | "stack";
  class?: string;
  style?: WidgetStyle;
  widget?: WidgetHints;
  children?: WidgetChild;
}

export function wBox(props: WBoxProps = {}): WidgetElement {
  return makeElement({
    kind: "box",
    axis: props.axis,
    class: props.class,
    style: props.style,
    widget: props.widget,
    children: flattenChildren(props.children),
  });
}

export function wRow(props: Omit<WBoxProps, "axis"> = {}): WidgetElement {
  return makeElement({
    kind: "row",
    axis: "row",
    class: props.class,
    style: props.style,
    widget: props.widget,
    children: flattenChildren(props.children),
  });
}

export function wCol(props: Omit<WBoxProps, "axis"> = {}): WidgetElement {
  return makeElement({
    kind: "col",
    axis: "column",
    class: props.class,
    style: props.style,
    widget: props.widget,
    children: flattenChildren(props.children),
  });
}

export function wStack(props: Omit<WBoxProps, "axis"> = {}): WidgetElement {
  return makeElement({
    kind: "stack",
    axis: "stack",
    class: props.class,
    style: props.style,
    widget: props.widget,
    children: flattenChildren(props.children),
  });
}

export interface WTextProps {
  class?: string;
  style?: WidgetStyle;
  numberOfLines?: number;
  widget?: WidgetHints;
  children?: WidgetChild;
}

export function wText(props: WTextProps = {}): WidgetElement {
  const { text, runs } = flattenTextRuns(props.children);
  return makeElement({
    kind: "text",
    text,
    runs,
    class: props.class,
    style: props.style,
    numberOfLines: props.numberOfLines,
    widget: props.widget,
  });
}

export interface WDateProps {
  date: number | Date;
  mode?: WidgetDateMode;
  class?: string;
  style?: WidgetStyle;
  widget?: WidgetHints;
}

export function wDate(props: WDateProps): WidgetElement {
  const dateMs = props.date instanceof Date ? props.date.getTime() : props.date;
  return makeElement({
    kind: "date",
    dateMs: Math.round(dateMs),
    mode: props.mode ?? "relative",
    class: props.class,
    style: props.style,
    widget: props.widget,
  });
}

export interface WImageProps {
  source: string;
  resizeMode?: WidgetResizeMode;
  class?: string;
  style?: WidgetStyle;
  widget?: WidgetHints;
}

export function wImage(props: WImageProps): WidgetElement {
  return makeElement({
    kind: "image",
    source: props.source,
    resizeMode: props.resizeMode,
    class: props.class,
    style: props.style,
    widget: props.widget,
  });
}

export interface WProgressProps {
  value: number;
  max?: number;
  class?: string;
  style?: WidgetStyle;
  trackClass?: string;
  trackStyle?: WidgetStyle;
  indicatorClass?: string;
  indicatorStyle?: WidgetStyle;
}

export function wProgress(props: WProgressProps): WidgetElement {
  return makeElement({
    kind: "progress",
    value: props.value,
    max: props.max,
    class: props.class,
    style: props.style,
    trackClass: props.trackClass,
    trackStyle: props.trackStyle,
    indicatorClass: props.indicatorClass,
    indicatorStyle: props.indicatorStyle,
  });
}

export interface WLinkProps {
  href: string;
  accessibilityLabel?: string;
  class?: string;
  style?: WidgetStyle;
  children?: WidgetChild;
}

export function wLink(props: WLinkProps): WidgetElement {
  return makeElement({
    kind: "link",
    href: props.href,
    accessibilityLabel: props.accessibilityLabel,
    class: props.class,
    style: props.style,
    children: flattenChildren(props.children),
  });
}

export interface WActionProps {
  action: string;
  payload?: unknown;
  patch?: Record<string, unknown>;
  accessibilityLabel?: string;
  class?: string;
  style?: WidgetStyle;
  children?: WidgetChild;
}

export function wAction(props: WActionProps): WidgetElement {
  return makeElement({
    kind: "action",
    action: props.action,
    payload: props.payload,
    patch: props.patch,
    accessibilityLabel: props.accessibilityLabel,
    class: props.class,
    style: props.style,
    children: flattenChildren(props.children),
  });
}

/** Declarative container node in a widget template. */
export function WBox(props: WBoxProps): WidgetElement {
  return wBox(props);
}

/** Horizontal stack container (`HStack` in SwiftUI). */
export function WRow(props: Omit<WBoxProps, "axis">): WidgetElement {
  return wRow(props);
}

/** Vertical stack container (`VStack` in SwiftUI). */
export function WCol(props: Omit<WBoxProps, "axis">): WidgetElement {
  return wCol(props);
}

/** Overlay stack container (`ZStack` in SwiftUI). */
export function WStack(props: Omit<WBoxProps, "axis">): WidgetElement {
  return wStack(props);
}

/** Styled text node (supports inline styled `WText` runs). */
export function WText(props: WTextProps): WidgetElement {
  return wText(props);
}

/** Live OS-driven timestamp/timer node (`SwiftUI Text(date, style:)`). */
export function WDate(props: WDateProps): WidgetElement {
  return wDate(props);
}

/** SF Symbol (`symbol:...`), bundled asset (`asset:/...`), or shared file (`file://...`). */
export function WImage(props: WImageProps): WidgetElement {
  return wImage(props);
}

/** Linear determinate progress bar. */
export function WProgress(props: WProgressProps): WidgetElement {
  return wProgress(props);
}

/** Sub-region deep link (`SwiftUI Link(destination:)`). */
export function WLink(props: WLinkProps): WidgetElement {
  return wLink(props);
}

/** Interactive desktop widget button (`SwiftUI Button(intent:)`). */
export function WAction(props: WActionProps): WidgetElement {
  return wAction(props);
}
