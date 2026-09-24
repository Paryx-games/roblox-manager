type IconProps = {
  name: string;
  tone?: "image" | "current-color";
};

function getCurrentColorIconContent(name: string) {
  switch (name) {
    case "delete":
      return (
        <>
          <path d="M10 11v6" />
          <path d="M14 11v6" />
          <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
          <path d="M3 6h18" />
          <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
        </>
      );
    case "kill":
      return (
        <>
          <path d="m12.5 17-.5-1-.5 1h1z" />
          <path d="M15 22a1 1 0 0 0 1-1v-1a2 2 0 0 0 1.56-3.25 8 8 0 1 0-11.12 0A2 2 0 0 0 8 20v1a1 1 0 0 0 1 1z" />
          <circle cx="15" cy="12" r="1" />
          <circle cx="9" cy="12" r="1" />
        </>
      );
    case "warning":
      return (
        <>
          <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" />
          <path d="M12 9v4" />
          <path d="M12 17h.01" />
        </>
      );
    case "check":
      return <path d="m5 12 4 4L19 6" />;
    case "close":
      return (
        <>
          <path d="M18 6 6 18" />
          <path d="m6 6 12 12" />
        </>
      );
    case "globe":
      return (
        <>
          <circle cx="12" cy="12" r="10" />
          <path d="M2 12h20" />
          <path d="M12 2a15.3 15.3 0 0 1 0 20" />
          <path d="M12 2a15.3 15.3 0 0 0 0 20" />
        </>
      );
    case "globe-off":
      return (
        <>
          <path d="M10.7 5.1a10 10 0 0 1 8.2 13.2" />
          <path d="M5.1 5.1a10 10 0 0 0 13.8 13.8" />
          <path d="M2 12h5" />
          <path d="M17 12h5" />
          <path d="M12 2v5" />
          <path d="M12 17v5" />
          <path d="m3 3 18 18" />
        </>
      );
    case "shield-question-mark":
      return (
        <>
          <path d="M20 13c0 5-3.5 7.5-8 8.5C7.5 20.5 4 18 4 13V6l8-3 8 3v7Z" />
          <path d="M9.1 9a3 3 0 1 1 5.8 1c0 2-3 2-3 4" />
          <path d="M12 17h.01" />
        </>
      );
    case "refresh":
      return (
        <>
          <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />
          <path d="M21 3v5h-5" />
          <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" />
          <path d="M8 16H3v5" />
        </>
      );
    default:
      return null;
  }
}

export function Icon({ name, tone = "image" }: IconProps) {
  if (tone === "current-color") {
    const content = getCurrentColorIconContent(name);
    if (content) {
      return (
        <svg
          className="account-icon account-icon-current-color"
          data-icon-name={name}
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          {content}
        </svg>
      );
    }
  }

  return (
    <img
      className="account-icon"
      src={`/icons/${name}.svg`}
      alt=""
      aria-hidden="true"
    />
  );
}
