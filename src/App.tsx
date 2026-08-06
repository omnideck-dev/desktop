import { useState } from "react";
import AppShell, { type View } from "./components/AppShell";
import BlockingScreen from "./components/BlockingScreen";
import DashboardView from "./components/dashboard/DashboardView";
import SettingsView from "./components/SettingsView";
import { useCliVersion } from "./hooks/useCliVersion";

export default function App() {
  const cliVersion = useCliVersion();
  const [view, setView] = useState<View>("dashboard");

  // Instant-open rule (AGENT.md): the window is already visible and this
  // renders a real default state immediately — never a blank pane while we
  // wait on the first backend round-trip.
  if (cliVersion.status === "checking") {
    return (
      <div className="checking-screen">
        <div className="spinner" aria-label="Working" />
        <p>Checking your setup…</p>
      </div>
    );
  }

  if (cliVersion.status === "error") {
    return <BlockingScreen error={cliVersion.error} />;
  }

  return (
    <AppShell view={view} onViewChange={setView}>
      {view === "dashboard" ? <DashboardView /> : <SettingsView cliVersion={cliVersion} />}
    </AppShell>
  );
}
