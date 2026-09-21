import type { CSSProperties } from "react";

type IconProps = {
  name: string;
  tone?: "image" | "current-color";
};

export function Icon({ name, tone = "image" }: IconProps) {
  if (tone === "current-color") {
    return (
      <span
        className="account-icon account-icon-current-color"
        data-icon-name={name}
        style={
          {
            WebkitMaskImage: `url("/icons/${name}.svg")`,
            WebkitMaskPosition: "center",
            WebkitMaskRepeat: "no-repeat",
            WebkitMaskSize: "contain",
            maskImage: `url("/icons/${name}.svg")`,
          } as CSSProperties
        }
        aria-hidden="true"
      />
    );
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
