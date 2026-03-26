import React from "react";
import {
  AbsoluteFill,
  useCurrentFrame,
  useVideoConfig,
  interpolate,
  spring,
} from "remotion";
import { Terminal } from "./Terminal";

export const Scene2Onboarding: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const titleSpring = spring({
    frame,
    fps,
    config: { damping: 200 },
  });

  const titleY = interpolate(titleSpring, [0, 1], [-50, 0]);
  const titleOpacity = titleSpring;

  const lines = [
    { type: "comment" as const, content: "# See the zsh widget", delay: 0.5 },
    { type: "input" as const, content: "lmc init zsh", delay: 1.0 },
    { type: "output" as const, content: "# lmc zsh widget", delay: 2.0 },
    { type: "output" as const, content: "function _lmc_widget() { ... }", delay: 2.2 },
    { type: "output" as const, content: "zle -N _lmc_widget", delay: 2.4 },
    { type: "output" as const, content: "bindkey '^[[Z' _lmc_widget", delay: 2.6 },
    { type: "comment" as const, content: "# Add to your shell", delay: 4.0 },
    { type: "input" as const, content: 'eval "$(lmc init zsh)"', delay: 4.5 },
    { type: "comment" as const, content: "# Now press Shift+Tab in your terminal!", delay: 6.0 },
    { type: "output" as const, content: "✓ Widget installed to ~/.zshrc", delay: 7.0 },
  ];

  return (
    <AbsoluteFill
      style={{
        backgroundColor: "#0d1117",
        flexDirection: "column",
      }}
    >
      {/* Title */}
      <div
        style={{
          position: "absolute",
          top: 40,
          left: 0,
          right: 0,
          textAlign: "center",
          transform: `translateY(${titleY}px)`,
          opacity: titleOpacity,
        }}
      >
        <h1
          style={{
            fontSize: 48,
            fontWeight: 700,
            color: "#ffffff",
            margin: 0,
            fontFamily: "system-ui, sans-serif",
          }}
        >
          Shell <span style={{ color: "#a6e3a1" }}>Onboarding</span>
        </h1>
        <p
          style={{
            fontSize: 24,
            color: "#8b949e",
            margin: "8px 0 0 0",
          }}
        >
          One line to your ~/.zshrc
        </p>
      </div>

      {/* Terminal */}
      <Terminal title="Terminal — zsh" lines={lines} startFrame={15} />
    </AbsoluteFill>
  );
};
