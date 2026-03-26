import React from "react";
import {
  AbsoluteFill,
  useCurrentFrame,
  useVideoConfig,
  interpolate,
  spring,
} from "remotion";
import { Terminal } from "./Terminal";

export const Scene6Stats: React.FC = () => {
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
    { type: "comment" as const, content: "# Usage is tracked locally", delay: 0.5 },
    { type: "input" as const, content: "lmc stats", delay: 1.0 },
    { type: "output" as const, content: "", delay: 2.0 },
    { type: "output" as const, content: "=== lmc Usage Stats ===", delay: 2.3 },
    { type: "output" as const, content: "", delay: 2.6 },
    { type: "output" as const, content: "Total requests:    42", delay: 3.0 },
    { type: "output" as const, content: "expand:            28", delay: 3.3 },
    { type: "output" as const, content: "explain:           14", delay: 3.6 },
    { type: "output" as const, content: "", delay: 4.0 },
    { type: "output" as const, content: "Last used:         2 minutes ago", delay: 4.3 },
    { type: "comment" as const, content: "", delay: 5.0 },
    { type: "comment" as const, content: "# Simple. Local. Non-intrusive.", delay: 5.5 },
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
          Local <span style={{ color: "#89dceb" }}>Stats</span>
        </h1>
        <p
          style={{
            fontSize: 24,
            color: "#8b949e",
            margin: "8px 0 0 0",
          }}
        >
          Your data stays on your machine
        </p>
      </div>

      {/* Terminal */}
      <Terminal title="Terminal — zsh" lines={lines} startFrame={15} />
    </AbsoluteFill>
  );
};
