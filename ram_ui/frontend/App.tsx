import { useState } from "react";

type NavItem = {
  label: string;
  icon: string;
};

const navItems: NavItem[] = [
  { label: "Instances", icon: "package" },
  { label: "Client Manager", icon: "app-window" },
  { label: "Accounts", icon: "id-card" },
];

function Icon({ name }: { name: string }) {
  return (
    <img
      className="h-full w-full object-contain"
      src={`/icons/${name}.svg`}
      alt=""
      aria-hidden="true"
    />
  );
}

function RailButton({
  label,
  icon,
  active = false,
  onClick,
}: NavItem & { active?: boolean; onClick: () => void }) {
  return (
    <button
      className={`rail-button ${active ? "is-active" : ""}`}
      type="button"
      aria-label={label}
      aria-current={active ? "page" : undefined}
      data-tip={label}
      onClick={onClick}
    >
      <span className="accent-bar" aria-hidden="true" />
      <span className="icon-box">
        <Icon name={icon} />
      </span>
    </button>
  );
}

function WindowButton({
  label,
  icon,
  onClick,
}: {
  label: string;
  icon: string;
  onClick: () => void;
}) {
  return (
    <button
      className="window-button"
      type="button"
      aria-label={label}
      data-tip={label}
      onClick={onClick}
    >
      <Icon name={icon} />
    </button>
  );
}

export function App() {
  const [activeNav, setActiveNav] = useState("Instances");

  return (
    <div className="flex h-screen min-h-0 flex-col overflow-hidden bg-canvas text-primary">
      <header className="titlebar">
        <span className="titlebar-logo" aria-label="Roblox Manager">
          <Icon name="feather" />
        </span>
        <span className="titlebar-version">1.3.7</span>
        <div className="window-controls" aria-label="Window controls">
          <WindowButton label="Minimize" icon="minus" onClick={() => {}} />
          <WindowButton label="Maximize" icon="square" onClick={() => {}} />
          <WindowButton label="Close" icon="close" onClick={() => {}} />
        </div>
      </header>

      <div className="body-row">
        <nav className="sidebar" aria-label="Primary navigation">
          {navItems.map((item) => (
            <RailButton
              {...item}
              key={item.label}
              active={activeNav === item.label}
              onClick={() => setActiveNav(item.label)}
            />
          ))}
          <span className="sidebar-spacer" />
          <RailButton label="Clear Cache" icon="eraser" onClick={() => {}} />
          <RailButton label="Settings" icon="settings" onClick={() => {}} />
        </nav>

        <div className="main-col">
          <div className="header-row">
            <h1 className="header-title">Client Manager</h1>
          </div>
          <main className="content">
            <section className="empty-state" aria-labelledby="whoops-title">
              <h2 id="whoops-title">Whoops!</h2>
              <p>
                You need to disable 'Auto-Pick Clients' inside the settings to
                use this feature.
              </p>
            </section>
          </main>
        </div>
      </div>
    </div>
  );
}
