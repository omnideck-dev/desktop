import { useState } from "react";
import AppShell, { COMMUNITY_URL, HELP_URL, instanceView, type InstanceTab, type View } from "./components/AppShell";
import BlockingScreen from "./components/BlockingScreen";
import DashboardView from "./components/dashboard/DashboardView";
import ExternalPage from "./components/ExternalPage";
import InstanceWebviewTab from "./components/InstanceWebviewTab";
import SettingsView from "./components/SettingsView";
import { useCliVersion } from "./hooks/useCliVersion";

export default function App() {
  const cliVersion = useCliVersion();
  const [view, setView] = useState<View>("dashboard");
  const [openTabs, setOpenTabs] = useState<InstanceTab[]>([]);

  function openInstance(name: string, port: string) {
    setOpenTabs((prev) => (prev.some((t) => t.name === name) ? prev : [...prev, { name, port }]));
    setView(instanceView(name));
  }

  function closeInstance(name: string) {
    setOpenTabs((prev) => prev.filter((t) => t.name !== name));
    setView((v) => (v === instanceView(name) ? "dashboard" : v));
  }

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

  const activeTab = openTabs.find((t) => view === instanceView(t.name));

  return (
    <AppShell view={view} onViewChange={setView} openTabs={openTabs} onCloseTab={closeInstance}>
      {view === "dashboard" && <DashboardView onOpenInstance={openInstance} />}
      {view === "settings" && <SettingsView cliVersion={cliVersion} />}
      {view === "help" && <ExternalPage title="Help" url={HELP_URL} />}
      {view === "community" && <ExternalPage title="Community" url={COMMUNITY_URL} />}
      {activeTab && <InstanceWebviewTab name={activeTab.name} port={activeTab.port} />}
    </AppShell>
  );
}
