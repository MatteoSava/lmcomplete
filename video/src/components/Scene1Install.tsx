import React from "react";
import {
  AbsoluteFill,
  useCurrentFrame,
  useVideoConfig,
  interpolate,
  spring,
} from "remotion";
import { Terminal } from "./Terminal";

export const Scene1Install: React.FC = () => {
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
    { type: "comment" as const, content: "# Install lmcomplete", delay: 0.5 },
    { type: "input" as const, content: "cargo install lmcomplete", delay: 1.5 },
    { type: "output" as const, content: "    Updating crates.io index", delay: 2.5 },
    { type: "output" as const, content: " Downloading lmcomplete v0.1.0", delay: 2.8 },
    { type: "output" as const, content: "  Compiling lmcomplete v0.1.0", delay: 3.2 },
    { type: "output" as const, content: "   Finished release [optimized] target(s)", delay: 4.0 },
    { type: "output" as const, content: "  Installing ~/.cargo/bin/lmc", delay: 4.5 },
    { type: "comment" as const, content: "# Check the binary", delay: 5.5 },
    { type: "input" as const, content: "lmc --version", delay: 6.0 },
    { type: "output" as const, content: "lmc 0.1.0", delay: 6.5 },
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
          <span style={{ color: "#89b4fa" }}>lmc</span> — Fast Install
        </h1>
        <p
          style={{
            fontSize: 24,
            color: "#8b949e",
            margin: "8px 0 0 0",
          }}
        >
          One command. Ready in seconds.
        </p>
      </div>

      {/* Terminal */}
      <Terminal title="Terminal — zsh" lines={lines} startFrame={15} />
    </AbsoluteFill>
  );
};
