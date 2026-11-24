import { mount, Scroll, signal, Text, UrlScheme, View } from "nsh";
import { Button, Progress } from "nsh/base";
import { Window } from "nsh/desktop";
import { Widget, WidgetPreview } from "nsh-widget";
import {
  type Holding,
  PortfolioWidget,
  type PortfolioWidgetData,
} from "./widgets/PortfolioWidget";

const PRESETS: Holding[] = [
  { symbol: "AAPL", name: "Apple Inc.", price: "$218.40", changePct: "+1.42%", positive: true },
  { symbol: "NVDA", name: "NVIDIA Corp.", price: "$142.80", changePct: "+3.15%", positive: true },
  { symbol: "MSFT", name: "Microsoft Corp.", price: "$428.10", changePct: "-0.38%", positive: false },
  { symbol: "TSLA", name: "Tesla Inc.", price: "$254.60", changePct: "+2.08%", positive: true },
];

function App() {
  const selected = signal(0);
  const progress = signal(0.72);
  const widgetAvailable = signal(false);
  const status = signal(
    "Running without a packaged App Group entitlement. Inspect live cards below or build `.appex` and run `nsh package --target macos --run` for real WidgetKit.",
  );
  const lastLink = signal<string | null>(UrlScheme.url());
  const events = signal<string[]>([]);

  const appendEvent = (entry: string) => {
    events.set([`${new Date().toLocaleTimeString()} · ${entry}`, ...events().slice(0, 5)]);
  };

  void Widget.available().then((ok) => {
    widgetAvailable.set(ok);
    if (ok) {
      const group = Widget.appGroup();
      status.set(
        `WidgetKit host connected (${group ?? "default"}) — updates sync directly to macOS Desktop & Notification Center.`,
      );
    }
  });

  UrlScheme.on("open", ({ url }) => {
    lastLink.set(url);
    appendEvent(`Deep link opened: ${url}`);
    const match = url.match(/^pulse:\/\/symbol\/([A-Z]+)/i);
    if (match) {
      const idx = PRESETS.findIndex((p) => p.symbol === match[1].toUpperCase());
      if (idx >= 0) selected.set(idx);
    }
  });

  Widget.on("action", (event) => {
    appendEvent(
      `Widget action: ${event.widgetId}/${event.action} ${JSON.stringify(event.payload ?? {})}`,
    );
    if (event.action === "refresh") {
      selected.set((selected() + 1) % PRESETS.length);
      void pushLiveData();
    }
  });

  const buildPayload = (): PortfolioWidgetData => {
    const active = PRESETS[selected()];
    return {
      symbol: active.symbol,
      name: active.name,
      price: active.price,
      changePct: active.changePct,
      positive: active.positive,
      goalProgress: progress(),
      updatedAt: Date.now(),
      watchlist: PRESETS.slice(0, 3),
    };
  };

  const pushLiveData = async () => {
    const payload = buildPayload();
    if (!(await Widget.available())) {
      appendEvent(`Preview update prepared for ${payload.symbol} (${payload.price})`);
      return;
    }
    try {
      await Widget.update(PortfolioWidget, payload);
      status.set(`Synced ${payload.symbol} (${payload.price}) to WidgetKit.`);
      appendEvent(`Widget.update("portfolio", ${payload.symbol})`);
    } catch (err) {
      status.set(`Update failed: ${String(err)}`);
    }
  };

  const pushTimeline = async () => {
    if (!(await Widget.available())) {
      appendEvent("Preview timeline prepared (3 entries)");
      return;
    }
    const now = Date.now();
    const entries = PRESETS.slice(0, 3).map((item, idx) => ({
      date: now + idx * 15 * 60 * 1000,
      relevance: idx === 0 ? 1.0 : 0.6,
      data: {
        ...buildPayload(),
        symbol: item.symbol,
        name: item.name,
        price: item.price,
        changePct: item.changePct,
        positive: item.positive,
      },
    }));
    try {
      await Widget.setTimeline(PortfolioWidget, entries, { reloadPolicy: "atEnd" });
      status.set("Scheduled 3-entry timeline (15 min intervals).");
      appendEvent('Widget.setTimeline("portfolio", 3 entries)');
    } catch (err) {
      status.set(`Timeline failed: ${String(err)}`);
    }
  };

  return (
    <Window id="main" title="Pulse Widgets">
      <Scroll class="flex-1 bg-background">
        <View class="flex-col gap-5 p-6">
          <View class="flex-row items-center justify-between">
            <View class="flex-col gap-1">
              <Text class="text-xl font-semibold text-foreground">Pulse Widgets</Text>
              <Text class="text-sm text-muted-foreground">
                Push live snapshots, schedule timelines, and preview Light/Dark/Tinted cards.
              </Text>
            </View>
            <View
              class={
                widgetAvailable()
                  ? "px-2.5 py-1 rounded-full bg-primary"
                  : "px-2.5 py-1 rounded-full bg-secondary"
              }
            >
              <Text
                class={
                  widgetAvailable()
                    ? "text-xs font-medium text-primary-foreground"
                    : "text-xs font-medium text-secondary-foreground"
                }
              >
                {widgetAvailable() ? "WidgetKit Ready" : "Preview Mode"}
              </Text>
            </View>
          </View>

          <View class="p-4 flex-col gap-4 rounded-xl bg-card border border-border">
            <View class="flex-row items-center justify-between">
              <Text class="text-sm font-medium text-foreground">Active Symbol</Text>
              <View class="flex-row gap-2">
                {PRESETS.map((item, idx) => (
                  <Button
                    class={
                      selected() === idx
                        ? "px-3 py-1.5 rounded-lg bg-primary"
                        : "px-3 py-1.5 rounded-lg bg-secondary border border-border"
                    }
                    onPress={() => {
                      selected.set(idx);
                      void pushLiveData();
                    }}
                  >
                    <Text
                      class={
                        selected() === idx
                          ? "text-xs font-semibold text-primary-foreground"
                          : "text-xs font-medium text-secondary-foreground"
                      }
                    >
                      {item.symbol}
                    </Text>
                  </Button>
                ))}
              </View>
            </View>

            <View class="flex-col gap-2">
              <View class="flex-row items-center justify-between">
                <Text class="text-sm font-medium text-foreground">
                  {`Daily Goal Progress (${Math.round(progress() * 100)}%)`}
                </Text>
                <View class="flex-row gap-2">
                  <Button
                    class="px-2.5 py-1 rounded-md bg-secondary border border-border"
                    onPress={() => progress.set(Math.max(0.1, +(progress() - 0.1).toFixed(2)))}
                  >
                    <Text class="text-xs font-medium text-secondary-foreground">-10%</Text>
                  </Button>
                  <Button
                    class="px-2.5 py-1 rounded-md bg-secondary border border-border"
                    onPress={() => progress.set(Math.min(1.0, +(progress() + 0.1).toFixed(2)))}
                  >
                    <Text class="text-xs font-medium text-secondary-foreground">+10%</Text>
                  </Button>
                </View>
              </View>
              <Progress
                value={Math.round(progress() * 100)}
                class="h-2 rounded-full bg-muted"
                indicatorClass="h-2 rounded-full bg-primary"
              />
            </View>

            <View class="flex-row gap-3 pt-1">
              <Button class="px-4 py-2 rounded-lg bg-primary" onPress={() => void pushLiveData()}>
                <Text class="text-xs font-semibold text-primary-foreground">Push Snapshot</Text>
              </Button>
              <Button class="px-4 py-2 rounded-lg bg-secondary" onPress={() => void pushTimeline()}>
                <Text class="text-xs font-medium text-secondary-foreground">
                  Schedule 3-Entry Timeline
                </Text>
              </Button>
              <Button
                class="px-4 py-2 rounded-lg bg-background border border-border"
                onPress={() => {
                  if (widgetAvailable()) {
                    void Widget.reload("portfolio");
                  }
                  appendEvent('Widget.reload("portfolio")');
                }}
              >
                <Text class="text-xs font-medium text-foreground">Reload Timelines</Text>
              </Button>
            </View>
          </View>

          <WidgetPreview
            widget={PortfolioWidget}
            data={buildPayload()}
            onAction={(ev) => {
              appendEvent(`Preview action: ${ev.action} ${JSON.stringify(ev.payload ?? {})}`);
              if (ev.action === "refresh") {
                selected.set((selected() + 1) % PRESETS.length);
              }
            }}
            onOpenUrl={(url) => {
              lastLink.set(url);
              appendEvent(`Preview deep link: ${url}`);
            }}
          />

          <View class="p-4 flex-col gap-2 rounded-xl bg-card border border-border">
            <View class="flex-row items-center justify-between">
              <Text class="text-sm font-semibold text-foreground">Activity & Deep Links</Text>
              <Text class="text-xs text-muted-foreground">
                {lastLink() ? `Last URL: ${lastLink()}` : "No deep link opened yet"}
              </Text>
            </View>
            <Text class="text-xs text-muted-foreground">{status()}</Text>
            <View class="flex-col gap-1.5 pt-2">
              {() =>
                events().length === 0 ? (
                  <Text class="text-xs text-muted-foreground">
                    Trigger an update above or click Refresh in the preview or desktop widget.
                  </Text>
                ) : (
                  events().map((line) => (
                    <Text class="text-xs font-mono text-foreground">{line}</Text>
                  ))
                )
              }
            </View>
          </View>
        </View>
      </Scroll>
    </Window>
  );
}

mount(App);
