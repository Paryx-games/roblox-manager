type SkeletonLayout = "accounts" | "presets" | "servers" | "inventory" | "inventory-grid" | "results" | "account" | "group" | "settings" | "status";

function SkeletonRow() {
  return <div className="loading-skeleton-row">
    <span className="loading-skeleton-block loading-skeleton-avatar" />
    <div className="loading-skeleton-copy">
      <span className="loading-skeleton-block loading-skeleton-title" />
      <span className="loading-skeleton-block loading-skeleton-text" />
    </div>
    <span className="loading-skeleton-block loading-skeleton-field" />
    <span className="loading-skeleton-block loading-skeleton-control" />
  </div>;
}

export function LoadingSkeleton({ layout, label, count = 4 }: { layout: SkeletonLayout; label: string; count?: number }) {
  const isPanelLayout = layout === "account" || layout === "group" || layout === "settings";
  return <div className={`loading-skeleton loading-skeleton-${layout}`} role="status" aria-label={label} aria-busy="true">
    <span className="loading-skeleton-announcement">{label}</span>
    {layout === "settings" && <div className="loading-skeleton-navigation" aria-hidden="true">
      {Array.from({ length: 8 }, (_, index) => <span key={index} className="loading-skeleton-block loading-skeleton-title" />)}
    </div>}
    {isPanelLayout ? <div className="loading-skeleton-panels" aria-hidden="true">
      {layout !== "settings" && <div className="loading-skeleton-profile"><SkeletonRow /></div>}
      {Array.from({ length: 3 }, (_, index) => <div className="loading-skeleton-panel" key={index}>
        <span className="loading-skeleton-block loading-skeleton-heading" />
        <span className="loading-skeleton-block loading-skeleton-text" />
        {Array.from({ length: layout === "settings" ? 4 : 3 }, (_, row) => <SkeletonRow key={row} />)}
      </div>)}
    </div> : layout === "inventory-grid" ? Array.from({ length: count }, (_, index) => <div className="inventories-skeleton" key={index} aria-hidden="true">
      <span className="loading-skeleton-block inventories-thumbnail" />
      <div className="loading-skeleton-tile-copy">
        <span className="loading-skeleton-block loading-skeleton-title" />
        <span className="loading-skeleton-block loading-skeleton-text" />
      </div>
      <span className="loading-skeleton-block loading-skeleton-identifier" />
    </div>) : <div className="loading-skeleton-rows" aria-hidden="true">
      {(layout === "accounts" || layout === "servers") && <span className="loading-skeleton-block loading-skeleton-heading" />}
      {Array.from({ length: count }, (_, index) => <SkeletonRow key={index} />)}
    </div>}
  </div>;
}
