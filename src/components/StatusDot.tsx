const STATUS_LABEL: Record<string, string> = {
  running: "Running",
  exited: "Stopped",
  paused: "Paused",
  unknown: "Unknown",
};

const STATUS_CLASS: Record<string, string> = {
  running: "status-dot--success",
  paused: "status-dot--warning",
  exited: "status-dot--neutral",
  unknown: "status-dot--danger",
};

export default function StatusDot({ status }: { status: string }) {
  const cls = STATUS_CLASS[status] ?? "status-dot--danger";
  const label = STATUS_LABEL[status] ?? status;
  return (
    <span className="status-dot-wrap">
      <span className={`status-dot ${cls}`} aria-hidden="true" />
      <span>{label}</span>
    </span>
  );
}
