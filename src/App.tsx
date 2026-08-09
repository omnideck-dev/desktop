import { useState } from "react";
import AppShell, { COMMUNITY_URL, HELP_URL, instanceView, type InstanceTab, type View } from "./components/AppShell";
import BlockingScreen from "./components/BlockingScreen";
import DashboardView from "./components/dashboard/DashboardView";
import ExternalPage from "./components/ExternalPage";
import InstanceWebviewTab from "./components/InstanceWebviewTab";
import OnboardingView from "./components/OnboardingView";
import SettingsView from "./components/SettingsView";
import { useBootstrap } from "./hooks/useBootstrap";
import { useCliVersion } from "./hooks/useCliVersion";

export default function App() {
  const cliVersion = useCliVersion();
  const bootstrap = useBootstrap(cliVersion.status === "ok");
  const [onboardingComplete, setOnboardingComplete] = useState(false);
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

  // Skip the onboarding screen entirely once the shared runtime was already
  // ready on the very first bootstrap check (the normal day-to-day launch)
  // — only a first run or a repair actually shows it. Once shown, it stays
  // dismissed only via the user's own "Continue" click (onboardingComplete),
  // not by the state simply reaching "ready" — see OnboardingView's doc
  // comment for why an automatic swap away from a just-finished screen
  // would be jarring.
  const showOnboarding = !bootstrap.initiallyReady && !onboardingComplete;
  if (showOnboarding) {
    if (!bootstrap.state) {
      return (
        <div className="checking-screen">
          <div className="spinner" aria-label="Working" />
          <p>Checking your setup…</p>
        </div>
      );
    }
    return (
      <OnboardingView
        state={bootstrap.state}
        actionError={bootstrap.actionError}
        actionPending={bootstrap.actionPending}
        onBeginSetup={bootstrap.beginSetup}
        onRunAction={bootstrap.runAction}
        onContinue={() => setOnboardingComplete(true)}
      />
    );
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
