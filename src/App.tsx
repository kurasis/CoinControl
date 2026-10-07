import { HashRouter, Route, Routes } from "react-router";
import { AppProvider } from "./app/AppContext";
import { Layout } from "./components/Layout";
import { ActivityPage } from "./pages/ActivityPage";
import { AssetPage } from "./pages/AssetPage";
import { PortfolioPage } from "./pages/PortfolioPage";
import { ReviewPage } from "./pages/ReviewPage";
import { AccountPage } from "./pages/AccountPage";
import { GroupPage, WalletPage } from "./pages/ScopedPage";
import { SettingsPage } from "./pages/SettingsPage";
import { WalletsPage } from "./pages/WalletsPage";

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
