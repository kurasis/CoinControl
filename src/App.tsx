import { lazy } from "react";
import { HashRouter, Route, Routes } from "react-router";
import { AppProvider } from "./app/AppContext";
import { Layout } from "./components/Layout";
import { PortfolioPage } from "./pages/PortfolioPage";
const ActivityPage = lazy(() =>
  import("./pages/ActivityPage").then((m) => ({ default: m.ActivityPage })),
);
const AssetPage = lazy(() => import("./pages/AssetPage").then((m) => ({ default: m.AssetPage })));
const ReviewPage = lazy(() =>
  import("./pages/ReviewPage").then((m) => ({ default: m.ReviewPage })),
);
const AccountPage = lazy(() =>
  import("./pages/AccountPage").then((m) => ({ default: m.AccountPage })),
);
const GroupPage = lazy(() => import("./pages/ScopedPage").then((m) => ({ default: m.GroupPage })));
const WalletPage = lazy(() =>
  import("./pages/ScopedPage").then((m) => ({ default: m.WalletPage })),
);
const SettingsPage = lazy(() =>
  import("./pages/SettingsPage").then((m) => ({ default: m.SettingsPage })),
);
const WalletsPage = lazy(() =>
  import("./pages/WalletsPage").then((m) => ({ default: m.WalletsPage })),
);

export function App() {
  return (
    <AppProvider>
      <HashRouter>
        <Routes>
          <Route element={<Layout />}>
            <Route index element={<PortfolioPage />} />
            <Route path="wallets" element={<WalletsPage />} />
            <Route path="wallets/:id" element={<WalletPage />} />
            <Route path="accounts/:id" element={<AccountPage />} />
            <Route path="activity" element={<ActivityPage />} />
            <Route path="assets/:assetId" element={<AssetPage />} />
            <Route path="review" element={<ReviewPage />} />
            <Route path="groups/:id" element={<GroupPage />} />
            <Route path="settings" element={<SettingsPage />} />
            <Route path="settings/:tab" element={<SettingsPage />} />
          </Route>
        </Routes>
      </HashRouter>
    </AppProvider>
  );
}
