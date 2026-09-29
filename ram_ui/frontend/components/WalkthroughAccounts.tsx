import { Icon } from "./Icon";

const demoAccounts = [
  { name: "builderman", displayName: "builderman" },
  { name: "roblox", displayName: "Roblox" },
] as const;

export type DemoAccountName = typeof demoAccounts[number]["name"];

function DemoAvatar({ name, large = false }: { name: DemoAccountName; large?: boolean }) {
  return <span className={`account-avatar ${large ? "account-avatar-large" : ""}`}>
    <span className="account-avatar-media"><img src={`/demo-avatars/${name}.png`} alt="" /></span>
  </span>;
}

export function WalkthroughAccounts({ selectedName, onSelect }: {
  selectedName: DemoAccountName | null;
  onSelect: (name: DemoAccountName) => void;
}) {
  const selectedAccount = demoAccounts.find((account) => account.name === selectedName);

  return <>
    <div className="header-row accounts-header-row">
      <h1 className="header-title">Accounts</h1>
      <span className="walkthrough-demo-badge">Showcase / demo accounts</span>
    </div>
    <main className="accounts-page">
      <aside className="accounts-list-panel" aria-label="Demo accounts" data-walkthrough="accounts">
        <div className="accounts-list-head">
          <div className="accounts-search-row">
            <label className="accounts-search-field"><Icon name="search" /><input placeholder="Search accounts" readOnly aria-label="Search demo accounts" /></label>
            <button className="icon-button bordered" type="button" aria-label="Add account" data-walkthrough="add-account"><Icon name="add" /></button>
          </div>
        </div>
        <div className="walkthrough-demo-list">
          <div className="walkthrough-demo-group"><Icon name="chevron-down" /><span>Demo accounts</span><span className="group-count">2</span></div>
          {demoAccounts.map((account) => <button
            key={account.name}
            className={`account-row ${selectedName === account.name ? "is-selected" : ""}`}
            type="button"
            aria-label={`Select ${account.displayName} demo account`}
            aria-pressed={selectedName === account.name}
            data-showcase-action="select-demo-account"
            onClick={() => onSelect(account.name)}
          >
            <span className="account-row-bar" aria-hidden="true" />
            <DemoAvatar name={account.name} />
            <span className="account-row-name">{account.displayName}</span>
            <span className="walkthrough-demo-badge">Demo</span>
          </button>)}
        </div>
      </aside>
      <section className="accounts-detail-panel" aria-label="Demo account details" data-walkthrough="account-details">
        {!selectedAccount ? <div className="accounts-detail-empty">
          <Icon name="id-card" /><h2>Select a demo account</h2>
          <p>Choose Builderman or Roblox to explore the launch controls. These accounts are examples with no saved credentials.</p>
        </div> : <>
          <section className="account-card account-profile-card">
            <DemoAvatar name={selectedAccount.name} large />
            <div className="account-profile-copy">
              <div className="account-identity-name"><h2>{selectedAccount.displayName}</h2><span className="walkthrough-demo-badge">Demo account</span></div>
              <div className="account-identity-username"><p>@{selectedAccount.name}</p></div>
              <p className="walkthrough-demo-label">Example only. No credentials are saved, and this account can't be edited, removed or launched.</p>
            </div>
            <div className="account-actions-menu-anchor">
              <button className="icon-button" type="button" aria-label="More account actions unavailable for demo account" disabled><Icon name="more" /></button>
            </div>
          </section>
          <div className="walkthrough-demo-actions">
            <button className="account-button" type="button" disabled><Icon name="pin" />Pin account</button>
            <button className="account-button" type="button" disabled><Icon name="delete-danger" />Remove account</button>
          </div>
          <section className="account-card launch-card" data-walkthrough="launch">
            <label htmlFor="demo-place-id">Place ID</label>
            <input id="demo-place-id" className="walkthrough-demo-place" value="123456789" readOnly />
            <p className="walkthrough-demo-label">Example destination. You don't need to enter anything.</p>
            <div className="account-action-row">
              <button className="account-button" type="button"><Icon name="launch" />Launch</button>
              <button className="account-button" type="button"><Icon name="browser" />Open browser</button>
            </div>
          </section>
        </>}
      </section>
    </main>
  </>;
}
