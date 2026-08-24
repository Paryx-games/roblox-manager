import { Link } from 'react-router-dom'
import { releases } from './generated/changelog'

export function FeaturesPage() {
  return (
    <main className="changelog-page">
      <div className="changelog-head">
        <p className="eyebrow">Every feature, every version</p>
        <h1>The full list.</h1>
        <p className="lead">
          Everything Roblox Manager can do, pulled straight from the changelog.
          Updated every single time something changes in the changelog
        </p>
        <Link className="secondary back-link" to="/">← Back home</Link>
      </div>

      <div className="changelog-list">
        {releases.map((release) => (
          <section className="release" key={release.version}>
            <div className="release-version">
              <span className="version-tag">v{release.version}</span>
            </div>

            <ul className="release-items">
              {release.items.map((item) => (
                <li key={item}>{item}</li>
              ))}
            </ul>
          </section>
        ))}
      </div>
    </main>
  )
}