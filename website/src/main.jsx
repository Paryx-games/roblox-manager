import { StrictMode, useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { HashRouter, Routes, Route, Link, useLocation, useNavigate } from 'react-router-dom'
import { FeaturesPage } from './FeaturesPage'
import './style.css'

const repo = 'https://github.com/Paryx-games/roblox-manager'
const logo = 'https://raw.githubusercontent.com/Paryx-games/roblox-manager/main/assets/Logo.png'

function scrollToFeatures() {
  document.getElementById('features')?.scrollIntoView({ behavior: 'smooth' })
}

const ACCOUNTS = [
  { name: 'mainbuilder22', state: 'In Studio', live: true },
  { name: 'alt_tester_04', state: 'In Game', live: true },
  { name: 'alt_tester_05', state: 'Online', live: true },
  { name: 'backup_qa', state: 'Offline', live: false },
]

// pulled from the real changelog - the 9 most distinctive things it does, not a generic feature list
const FEATURES = [
  {
    title: 'Multi-account, no re-login',
    desc: "Every account stays signed in at once. Switch between them without logging out first.",
    called_out: true,
  },
  {
    title: 'Windows matched to the right account',
    desc: "RM reads a token off each client's own command line, so bulk launches don't mix accounts up — even a Roblox you started yourself is never mistaken for one of RM's.",
  },
  {
    title: 'Join the server another account is in',
    desc: 'Right-click an account that\'s currently in a game, and launch straight into that same server.',
  },
  {
    title: 'Multi-monitor window tiling',
    desc: 'Six layout modes across any connected display — Auto Grid, Fixed Columns/Rows, Custom Grid, Side-by-Side, or Stacked — with adjustable padding.',
  },
  {
    title: 'No master password required',
    desc: 'New installs encrypt the account store with a key held in Windows Credential Manager. The file on disk is still AES-256-GCM.',
  },
  {
    title: 'Moderation detection',
    desc: "Periodic checks flag banned or moderated accounts with the specific reason and expiry, plus a shortcut to open a browser and appeal.",
  },
  {
    title: 'Bulk asset uploads',
    desc: 'Upload decals, audio, models, animations, and video from any saved account, with moderation status tracked across restarts.',
  },
  {
    title: 'Group management',
    desc: 'Look up any group, browse its wall and roles, and join or leave it for every selected account at once.',
  },
  {
    title: 'Logs scrubbed of your credentials',
    desc: 'Cookies, auth tickets, CSRF tokens, and your Windows username never touch the log file. Saves are atomic, so a crash can\'t corrupt your account data.',
  },
]

const STACK = [
  { name: 'Rust', desc: 'Core + desktop application' },
  { name: 'egui', desc: 'Interface' },
  { name: 'Windows', desc: '10 / 11' },
]

const COMMANDS = [
  'git clone https://github.com/Paryx-games/roblox-manager.git',
  'cd roblox-manager',
  'cargo build --release',
]

function StatusStack() {
  return (
    <div className="status-stack" aria-hidden="true">
      <div className="status-stack-head">accounts.rm</div>
      {ACCOUNTS.map((a) => (
        <div className="status-row" key={a.name}>
          <span className={`dot ${a.live ? 'dot-live' : ''}`} />
          <span className="status-name">{a.name}</span>
          <span className="status-state">{a.state}</span>
        </div>
      ))}
    </div>
  )
}

function Header({ dark, setDark }) {
  const [navOpen, setNavOpen] = useState(false)
  const location = useLocation()
  const navigate = useNavigate()

  return (
    <header>
      <Link className="brand" to="/" onClick={() => setNavOpen(false)}>
        <img src={logo} alt="" onError={(e) => { e.currentTarget.style.display = 'none' }} />
        <span>Roblox Manager</span>
      </Link>

      <nav className={navOpen ? 'nav-open' : ''}>
        <button
          className="nav-link"
          onClick={() => {
            setNavOpen(false)
            if (location.pathname !== '/') {
              navigate('/')
              setTimeout(() => scrollToFeatures(), 50)
            } else {
              scrollToFeatures()
            }
          }}
        >
          Features
        </button>
        <Link
          to="/features"
          onClick={() => setNavOpen(false)}
          aria-current={location.pathname === '/features' ? 'page' : undefined}
        >
          Full changelog
        </Link>
        <a href={repo} target="_blank" rel="noreferrer">GitHub ↗</a>
        <button
          className="theme"
          onClick={() => setDark(!dark)}
          aria-label={dark ? 'Switch to light theme' : 'Switch to dark theme'}
          aria-pressed={dark}
        >
          {dark ? '☀' : '☾'}
        </button>
      </nav>

      <button
        className="menu-toggle"
        onClick={() => setNavOpen(!navOpen)}
        aria-label={navOpen ? 'Close menu' : 'Open menu'}
        aria-expanded={navOpen}
      >
        {navOpen ? '✕' : '☰'}
      </button>
    </header>
  )
}

function Footer() {
  return (
    <footer>
      <span>Roblox Manager — built by paryx</span>
      <a href={repo} target="_blank" rel="noreferrer">GitHub ↗</a>
    </footer>
  )
}

function HomePage() {
  const [copied, setCopied] = useState(false)

  function copyCommands() {
    navigator.clipboard.writeText(COMMANDS.join('\n')).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 1800)
    })
  }

  return (
    <main>
      <section className="hero">
        <div className="hero-copy">
          <p className="eyebrow">A small tool I built for myself</p>
          <h1>Stop logging<br />in and out.</h1>
          <p className="lead">
            I run a Roblox studio and got tired of switching accounts to test builds.
            So I wrote a desktop app that keeps every account signed in, side by side,
            in one window.
          </p>
          <div className="actions">
            <a className="primary" href={repo} target="_blank" rel="noreferrer">View on GitHub</a>
            <button className="secondary" onClick={scrollToFeatures}>What it does</button>
          </div>
          <p className="hero-note">Free. Open source. Windows only, for now.</p>
        </div>

        <div className="hero-visual">
          <StatusStack />
        </div>
      </section>

      <section id="features" className="features">
        <div className="section-head">
          <p className="eyebrow">What's in it</p>
          <h2>Nine things worth knowing about.<br />One of them is why it exists.</h2>
        </div>

        <div className="grid">
          {FEATURES.map((f) => (
            <article key={f.title} className={f.called_out ? 'called-out' : ''}>
              {f.called_out && <span className="callout-mark">the reason I built this</span>}
              <h3>{f.title}</h3>
              <p>{f.desc}</p>
            </article>
          ))}
        </div>

        <Link className="see-all" to="/features">See the full feature list ↗</Link>
      </section>

      <section id="stack" className="stack">
        <div>
          <p className="eyebrow">Built with</p>
          <h2>Rust, egui, and not much else.</h2>
          <p>No Electron. No background services phoning home. It's a native window that opens fast and gets out of your way.</p>
        </div>
        <div className="stack-list">
          {STACK.map((s) => (
            <div key={s.name}><strong>{s.name}</strong><span>{s.desc}</span></div>
          ))}
        </div>
      </section>

      <section className="install">
        <div>
          <p className="eyebrow">Building it yourself</p>
          <h2>Three commands.<br />That's the whole setup.</h2>
        </div>
        <div className="code-block">
          <code>
            {COMMANDS.map((c, i) => <span key={c} className="step">
              <span className="step-num">{i + 1}</span>{c}
            </span>)}
          </code>
          <button className="copy-btn" onClick={copyCommands} aria-label="Copy commands">
            {copied ? 'Copied' : 'Copy'}
          </button>
        </div>
      </section>

      <section className="warning">
        <strong>Before you use it</strong>
        <p>
          Roblox Manager isn't affiliated with or endorsed by Roblox Corporation.
          It interacts with Roblox authentication and local processes directly —
          use it at your own risk, and never share your .ROBLOSECURITY cookie with anyone.
        </p>
      </section>
    </main>
  )
}

function App() {
  const [dark, setDark] = useState(() => document.documentElement.dataset.theme !== 'light')

  useEffect(() => {
    document.documentElement.dataset.theme = dark ? 'dark' : 'light'
    localStorage.setItem('rm-theme', dark ? 'dark' : 'light')
  }, [dark])

  return (
    <div className="site">
      <Header dark={dark} setDark={setDark} />
      <Routes>
        <Route path="/" element={<HomePage />} />
        <Route path="/features" element={<FeaturesPage />} />
      </Routes>
      <Footer />
    </div>
  )
}

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <HashRouter>
      <App />
    </HashRouter>
  </StrictMode>,
)