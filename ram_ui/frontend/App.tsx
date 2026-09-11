import { useEffect, useState, type MouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AccountsPage } from "./AccountsPage";
import { GroupsPage } from "./GroupsPage";
import { PrivateServersPage } from "./PrivateServersPage";
import { recordUiDiagnostic } from "./lib/ipc";

type NavItem = {
  label: string;
  icon: string;
  page?: PageName;
};

const pageOrder = ["Accounts", "Groups", "Private Servers"] as const;
type PageName = (typeof pageOrder)[number];

const navItems: NavItem[] = [
  { label: "Accounts", icon: "id-card", page: "Accounts" },
  { label: "Groups", icon: "users", page: "Groups" },
  { label: "Private Servers", icon: "game", page: "Private Servers" },
];

type PageTransition = {
  outgoing: PageName;
  incoming: PageName;
  direction: "forward" | "backward";
};

const pageTransitionDurationMs = 240;

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
  disabled = false,
  onClick,
}: NavItem & { active?: boolean; disabled?: boolean; onClick: () => void }) {
  return (
    <button
      className={`rail-button ${active ? "is-active" : ""}`}
      type="button"
      aria-label={label}
      aria-current={active ? "page" : undefined}
      data-tip={label}
      disabled={disabled}
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
  close = false,
  onClick,
}: {
  label: string;
  icon: string;
  close?: boolean;
  onClick: () => Promise<void>;
}) {
  return (
    <button
      className={`window-button ${close ? "window-button-close" : ""}`}
      type="button"
      aria-label={label}
      data-tip={label}
      onMouseDown={(event) => event.stopPropagation()}
      onClick={onClick}
    >
      <Icon name={icon} />
    </button>
  );
}

export function App() {
  const [activeNav, setActiveNav] = useState<PageName>("Accounts");
  const [pageTransition, setPageTransition] = useState<PageTransition | null>(
    null,
  );
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());

  useEffect(() => {
    if (!pageTransition) {
      return;
    }

    const timeoutId = window.setTimeout(() => {
      setPageTransition(null);
    }, pageTransitionDurationMs);

    return () => window.clearTimeout(timeoutId);
  }, [pageTransition]);

  useEffect(() => {
    function recoverTransientUi(event: KeyboardEvent) {
      if (event.key !== "Escape" || !pageTransition) {
        return;
      }

      setPageTransition(null);
      void recordUiDiagnostic("page-transition-recovery");
    }

    document.addEventListener("keydown", recoverTransientUi);
    return () => document.removeEventListener("keydown", recoverTransientUi);
  }, [pageTransition]);

  function navigateTo(nextPage: PageName) {
    if (nextPage === activeNav) {
      return;
    }

    const direction =
      pageOrder.indexOf(nextPage) > pageOrder.indexOf(activeNav)
        ? "forward"
        : "backward";

    setPageTransition({
      outgoing: activeNav,
      incoming: nextPage,
      direction,
    });
    setActiveNav(nextPage);
  }

  function renderPage(page: PageName) {
    if (page === "Accounts") {
      return (
        <AccountsPage
          selectedIds={selectedIds}
          setSelectedIds={setSelectedIds}
        />
      );
    }

    if (page === "Groups") {
      return (
        <GroupsPage
          selectedIds={selectedIds}
          setSelectedIds={setSelectedIds}
          onNavigateAccounts={() => navigateTo("Accounts")}
        />
      );
    }

    if (page === "Private Servers") {
      return <PrivateServersPage selectedIds={selectedIds} />;
    }

    return (
      <>
        <div className="header-row">
          <h1 className="header-title">{page}</h1>
        </div>
        <main className="content">
          <section className="empty-state" aria-labelledby="whoops-title">
            <h2 id="whoops-title">Whoops!</h2>
            <p>
              You need to disable 'Auto-Pick Clients' inside the settings to use
              this feature.
            </p>
          </section>
        </main>
      </>
    );
  }

  function handleTitlebarMouseDown(event: MouseEvent<HTMLElement>) {
    const target = event.target;
    const clickedButton = target instanceof Element && target.closest("button");

    if (event.button === 0 && !clickedButton) {
      void getCurrentWindow().startDragging();
    }
  }

  function handleTitlebarDoubleClick(event: MouseEvent<HTMLElement>) {
    const target = event.target;
    const clickedButton = target instanceof Element && target.closest("button");

    if (!clickedButton) {
      void getCurrentWindow().toggleMaximize();
    }
  }

  return (
    <div className="flex h-screen min-h-0 flex-col overflow-hidden bg-canvas text-primary">
      <header
        className="titlebar"
        onMouseDown={handleTitlebarMouseDown}
        onDoubleClick={handleTitlebarDoubleClick}
      >
        <span className="titlebar-logo" aria-label="Roblox Manager">
          <Icon name="feather" />
        </span>
        <span className="titlebar-version">
          {import.meta.env.VITE_RM_VERSION}
        </span>
        <div className="window-controls" aria-label="Window controls">
          <WindowButton
            label="Minimize"
            icon="minus"
            onClick={() => getCurrentWindow().minimize()}
          />
          <WindowButton
            label="Maximize"
            icon="square"
            onClick={() => getCurrentWindow().toggleMaximize()}
          />
          <WindowButton
            label="Close"
            icon="close"
            close
            onClick={() => getCurrentWindow().close()}
          />
        </div>
      </header>

      <div className="body-row">
        <nav className="sidebar" aria-label="Primary navigation">
          {navItems.map((item) => (
            <RailButton
              {...item}
              key={item.label}
              active={activeNav === item.label}
              onClick={() => {
                if (item.page) {
                  navigateTo(item.page);
                }
              }}
            />
          ))}
          <span className="sidebar-spacer" />
          <RailButton
            label="Instances"
            icon="package"
            disabled
            onClick={() => {}}
          />
          <RailButton label="Clear Cache" icon="eraser" onClick={() => {}} />
          <RailButton label="Settings" icon="settings" onClick={() => {}} />
        </nav>

        <div className="main-col">
          <div className="page-transition-viewport">
            {pageTransition ? (
              <>
                <div
                  className={`page-transition-layer page-transition-outgoing page-transition-${pageTransition.direction}-out`}
                  aria-hidden="true"
                  ref={(element) => element?.setAttribute("inert", "")}
                >
                  {renderPage(pageTransition.outgoing)}
                </div>
                <div
                  className={`page-transition-layer page-transition-incoming page-transition-${pageTransition.direction}-in`}
                  aria-hidden="false"
                  data-interactive="true"
                >
                  {renderPage(pageTransition.incoming)}
                </div>
              </>
            ) : (
              <div className="page-transition-layer">
                {renderPage(activeNav)}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
