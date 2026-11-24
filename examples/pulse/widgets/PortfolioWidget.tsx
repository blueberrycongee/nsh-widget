import {
  defineWidget,
  WAction,
  WCol,
  WDate,
  WLink,
  WProgress,
  WRow,
  WText,
} from "nsh-widget";

export interface Holding {
  symbol: string;
  name: string;
  price: string;
  changePct: string;
  positive: boolean;
}

export interface PortfolioWidgetData {
  symbol: string;
  name: string;
  price: string;
  changePct: string;
  positive: boolean;
  goalProgress: number;
  updatedAt: number;
  watchlist: Holding[];
}

export const DEFAULT_PORTFOLIO_DATA: PortfolioWidgetData = {
  symbol: "AAPL",
  name: "Apple Inc.",
  price: "$218.40",
  changePct: "+1.42%",
  positive: true,
  goalProgress: 0.72,
  updatedAt: 1795280000000,
  watchlist: [
    { symbol: "AAPL", name: "Apple", price: "$218.40", changePct: "+1.42%", positive: true },
    { symbol: "NVDA", name: "NVIDIA", price: "$142.80", changePct: "+3.15%", positive: true },
    { symbol: "MSFT", name: "Microsoft", price: "$428.10", changePct: "-0.38%", positive: false },
  ],
};

export const PortfolioWidget = defineWidget<PortfolioWidgetData>({
  id: "portfolio",
  name: "Portfolio",
  families: ["small", "medium", "large"],
  placeholder: DEFAULT_PORTFOLIO_DATA,
  url: ({ data }) => `pulse://symbol/${data.symbol}`,
  render({ data, family }) {
    const item = data;
    const deltaClass = item.positive
      ? "text-xs font-semibold text-emerald-500"
      : "text-xs font-semibold text-destructive";

    return (
      <WCol class="flex-1 justify-between p-4 bg-card">
        <WRow class="items-center justify-between gap-2">
          <WCol>
            <WText class="text-sm font-semibold text-foreground">{item.symbol}</WText>
            {family !== "small" && (
              <WText class="text-xs text-muted-foreground">{item.name}</WText>
            )}
          </WCol>
          <WAction
            action="refresh"
            payload={{ symbol: item.symbol }}
            accessibilityLabel={`Refresh ${item.symbol} quote`}
            class="px-2.5 py-1 rounded-md bg-secondary"
          >
            <WText class="text-xs font-medium text-secondary-foreground">Refresh</WText>
          </WAction>
        </WRow>

        <WCol class="gap-1">
          <WText class="text-2xl font-bold text-foreground">{item.price}</WText>
          <WText class={deltaClass} widget={{ accentable: true }}>
            {item.changePct}
          </WText>
        </WCol>

        {family === "large" && (
          <WCol class="gap-2 py-2">
            {item.watchlist.map((row) => (
              <WLink
                href={`pulse://symbol/${row.symbol}`}
                class="flex-row items-center justify-between px-2.5 py-1.5 rounded-lg bg-secondary"
              >
                <WCol>
                  <WText class="text-xs font-semibold text-foreground">{row.symbol}</WText>
                  <WText class="text-[11px] text-muted-foreground">{row.name}</WText>
                </WCol>
                <WCol class="items-end">
                  <WText class="text-xs font-semibold text-foreground">{row.price}</WText>
                  <WText
                    class={
                      row.positive
                        ? "text-[11px] font-medium text-emerald-500"
                        : "text-[11px] font-medium text-destructive"
                    }
                    widget={{ accentable: true }}
                  >
                    {row.changePct}
                  </WText>
                </WCol>
              </WLink>
            ))}
          </WCol>
        )}

        {family !== "small" && (
          <WCol class="gap-2">
            <WProgress
              value={Math.round(item.goalProgress * 100)}
              class="h-1.5 rounded-full bg-muted"
              indicatorClass="h-1.5 rounded-full bg-primary"
            />
            <WRow class="items-center justify-between">
              <WLink href={`pulse://symbol/${item.symbol}/chart`} class="text-xs text-primary">
                <WText class="text-xs font-medium text-primary">Open Chart</WText>
              </WLink>
              <WDate
                date={item.updatedAt}
                mode="relative"
                class="text-xs text-muted-foreground"
              />
            </WRow>
          </WCol>
        )}
      </WCol>
    );
  },
});
