import React from "react";
import {
  AbsoluteFill,
  useCurrentFrame,
  useVideoConfig,
  interpolate,
  spring,
} from "remotion";
import { Terminal } from "./Terminal";

export const Scene3Expansion: React.FC = () => {
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
    { type: "comment" as const, content: "# Natural language → shell command", delay: 0.5 },
    { type: "input" as const, content: 'lmc "show git status"', delay: 1.5 },
    { type: "output" as const, content: "git status", delay: 3.0 },
    { type: "comment" as const, content: "", delay: 4.0 },
    { type: "comment" as const, content: "# Context-aware: knows your git state", delay: 5.0 },
    { type: "input" as const, content: 'lmc expand "commit all changes with message fix login"', delay: 5.5 },
    { type: "output" as const, content: 'git commit -am "fix login"', delay: 7.5 },
    { type: "comment" as const, content: "", delay: 8.5 },
    { type: "comment" as const, content: "# Output is a command, not prose", delay: 9.0 },
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
          Context-Aware <span style={{ color: "#f9e2af" }}>Expansion</span>
        </h1>
        <p
          style={{
            fontSize: 24,
            color: "#8b949e",
            margin: "8px 0 0 0",
          }}
        >
          Your shell, your git, your context
        </p>
      </div>

      {/* Terminal */}
      <Terminal title="Terminal — ~/project (git)" lines={lines} startFrame={15} />
    </AbsoluteFill>
  );
};
