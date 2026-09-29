import { Icon } from "./Icon";

const demoAccounts = [
  { name: "builderman", displayName: "Builderman" },
  { name: "roblox", displayName: "Roblox" },
] as const;

export type DemoAccountName = typeof demoAccounts[number]["name"];

export function WalkthroughAccounts({ selectedName, onSelect }: {
  selectedName: DemoAccountName | null;
  onSelect: (name: DemoAccountName) => void;
}) {
  const selectedAccount = demoAccounts.find((account) => account.name === selectedName);

  return <>
    <div className="header-row accounts-header-row">
      <h1 className="header-title">Accounts</h1>
      <span className="walkthrough-demo-label">Showcase / demo accounts only</span>
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
          {demoAccounts.map((account) => <button
            key={account.name}
            className={`account-row ${selectedName === account.name ? "is-selected" : ""}`}
            type="button"
            aria-label={`Select ${account.displayName} demo account`}
            aria-pressed={selectedName === account.name}
            data-showcase-action="select-demo-account"
            onClick={() => onSelect(account.name)}
          >
            <span className="walkthrough-demo-avatar" aria-hidden="true"><Icon name="id-card" /></span>
            <span className="account-row-name">{account.displayName}</span>
            <span className="walkthrough-demo-label">Demo</span>
          </button>)}
        </div>
      </aside>
      <section className="accounts-detail-panel" aria-label="Demo account details" data-walkthrough="account-details">
        {!selectedAccount ? <div className="accounts-detail-empty">
          <Icon name="id-card" /><h2>Select a demo account</h2>
          <p>Choose Builderman or Roblox to explore the launch controls. These accounts are examples with no saved credentials.</p>
        </div> : <>
          <section className="account-card account-profile-card">
            <span className="walkthrough-demo-avatar" aria-hidden="true"><Icon name="id-card" /></span>
            <div className="account-profile-copy">
              <div className="account-identity-name"><h2>{selectedAccount.displayName}</h2></div>
              <div className="account-identity-username"><p>@{selectedAccount.name}</p></div>
              <p className="walkthrough-demo-label">Demo account / no credentials, network requests or Roblox launch</p>
            </div>
          </section>
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
