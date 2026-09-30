type SkeletonLayout = "accounts" | "presets" | "servers" | "inventory" | "inventory-grid" | "filters" | "results" | "group-results" | "account" | "group" | "settings" | "status";

function SkeletonGroupText() {
  return <div className="loading-skeleton-group-lines">
    <span className="loading-skeleton-block" />
    <span className="loading-skeleton-block" />
    <span className="loading-skeleton-block" />
  </div>;
}

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
  const isPanelLayout = layout === "account" || layout === "settings";
  return <div className={`loading-skeleton loading-skeleton-${layout}`} role="status" aria-label={label} aria-busy="true">
    <span className="loading-skeleton-announcement">{label}</span>
    {layout === "settings" && <div className="loading-skeleton-navigation" aria-hidden="true">
      {Array.from({ length: 8 }, (_, index) => <span key={index} className="loading-skeleton-block loading-skeleton-title" />)}
    </div>}
    {layout === "group" ? <div className="loading-skeleton-group-content" aria-hidden="true">
      <div className="groups-card">
        <div className="groups-identity">
          <span className="groups-avatar-large"><span className="loading-skeleton-block loading-skeleton-group-icon" /></span>
          <div className="loading-skeleton-copy">
            <span className="loading-skeleton-block loading-skeleton-title" />
            <span className="loading-skeleton-block loading-skeleton-text" />
          </div>
        </div>
        <div className="groups-chip-row">
          <span className="loading-skeleton-block loading-skeleton-group-chip" />
          <span className="loading-skeleton-block loading-skeleton-group-chip" />
        </div>
        <div className="groups-chip-row">
          <span className="loading-skeleton-block loading-skeleton-group-chip" />
          <span className="loading-skeleton-block loading-skeleton-group-chip" />
        </div>
      </div>
      <div className="groups-card">
        <span className="loading-skeleton-block loading-skeleton-group-heading">About</span>
        <div className="groups-about"><SkeletonGroupText /></div>
        <div className="groups-about-meta">
          <span className="loading-skeleton-block loading-skeleton-text" />
        </div>
      </div>
      <div className="groups-card">
        <span className="loading-skeleton-block loading-skeleton-group-heading loading-skeleton-group-heading-with-icon">Announcement</span>
        <div className="groups-post groups-announcement">
          <span className="loading-skeleton-block loading-skeleton-title" />
          <SkeletonGroupText />
          <span className="loading-skeleton-block loading-skeleton-text" />
        </div>
      </div>
      <div className="groups-card">
        <span className="loading-skeleton-block loading-skeleton-group-heading loading-skeleton-group-heading-with-icon">Forums</span>
        <div className="groups-forum-list">
          <div className="groups-forum">
            <span className="loading-skeleton-block loading-skeleton-title" />
            <div className="groups-post">
              <SkeletonGroupText />
              <span className="loading-skeleton-block loading-skeleton-text" />
            </div>
          </div>
        </div>
      </div>
      <div className="groups-card">
        <div className="groups-card-head">
          <span className="loading-skeleton-block loading-skeleton-group-heading">Selected account membership</span>
          <span className="loading-skeleton-block loading-skeleton-group-count" />
        </div>
        <div className="groups-membership-list">
          {Array.from({ length: count }, (_, index) => <div className="groups-membership" key={index}>
            <span className="groups-avatar-small"><span className="loading-skeleton-block loading-skeleton-group-icon" /></span>
            <div className="loading-skeleton-copy"><span className="loading-skeleton-block loading-skeleton-title" /></div>
            <span className="loading-skeleton-block loading-skeleton-text" />
          </div>)}
          {count === 0 && <span className="loading-skeleton-block loading-skeleton-title" />}
        </div>
      </div>
    </div> : layout === "group-results" ? <div className="groups-results-page" aria-hidden="true">
      <div className="groups-results-header">
        <div className="loading-skeleton-copy">
          <span className="loading-skeleton-block loading-skeleton-group-heading">Search results</span>
          <span className="loading-skeleton-block loading-skeleton-text" />
        </div>
        <span className="loading-skeleton-block loading-skeleton-group-chip" />
      </div>
      <div className="groups-results">
        {Array.from({ length: count }, (_, index) => <div className="groups-result" key={index}>
          <span className="loading-skeleton-block loading-skeleton-title" />
          <span className="loading-skeleton-block loading-skeleton-text" />
          <span className="loading-skeleton-block loading-skeleton-title" />
          <span className="loading-skeleton-block loading-skeleton-text" />
        </div>)}
      </div>
    </div> : isPanelLayout ? <div className="loading-skeleton-panels" aria-hidden="true">
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
