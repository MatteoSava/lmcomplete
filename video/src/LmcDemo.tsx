import {
  AbsoluteFill,
  useVideoConfig,
} from "remotion";
import { TransitionSeries, linearTiming } from "@remotion/transitions";
import { fade } from "@remotion/transitions/fade";
import { Scene1Install } from "./components/Scene1Install";
import { Scene2Onboarding } from "./components/Scene2Onboarding";
import { Scene3Expansion } from "./components/Scene3Expansion";
import { Scene4Explanation } from "./components/Scene4Explanation";
import { Scene5Trust } from "./components/Scene5Trust";
import { Scene6Stats } from "./components/Scene6Stats";

export const LmcDemo: React.FC = () => {
  const { fps } = useVideoConfig();

  const transitionDuration = Math.round(0.5 * fps);

  // Scene durations based on max delay + 2s buffer for text to finish
  const scene1Duration = 8 * fps;  // max delay 6.5s
  const scene2Duration = 9 * fps;  // max delay 7.0s
  const scene3Duration = 11 * fps; // max delay 9.0s
  const scene4Duration = 9 * fps;  // max delay 7.0s
  const scene5Duration = 10 * fps; // max delay 8.0s
  const scene6Duration = 7 * fps;  // max delay 5.5s

  return (
    <AbsoluteFill style={{ backgroundColor: "#0d1117" }}>
      <TransitionSeries>
        {/* Scene 1: Install and identity */}
        <TransitionSeries.Sequence durationInFrames={scene1Duration}>
          <Scene1Install />
        </TransitionSeries.Sequence>

        <TransitionSeries.Transition
          presentation={fade()}
          timing={linearTiming({ durationInFrames: transitionDuration })}
        />

        {/* Scene 2: Shell onboarding */}
        <TransitionSeries.Sequence durationInFrames={scene2Duration}>
          <Scene2Onboarding />
        </TransitionSeries.Sequence>

        <TransitionSeries.Transition
          presentation={fade()}
          timing={linearTiming({ durationInFrames: transitionDuration })}
        />

        {/* Scene 3: Context-aware expansion */}
        <TransitionSeries.Sequence durationInFrames={scene3Duration}>
          <Scene3Expansion />
        </TransitionSeries.Sequence>

        <TransitionSeries.Transition
          presentation={fade()}
          timing={linearTiming({ durationInFrames: transitionDuration })}
        />

        {/* Scene 4: Explanation */}
        <TransitionSeries.Sequence durationInFrames={scene4Duration}>
          <Scene4Explanation />
        </TransitionSeries.Sequence>

        <TransitionSeries.Transition
          presentation={fade()}
          timing={linearTiming({ durationInFrames: transitionDuration })}
        />

        {/* Scene 5: Trust and redaction */}
        <TransitionSeries.Sequence durationInFrames={scene5Duration}>
          <Scene5Trust />
        </TransitionSeries.Sequence>

        <TransitionSeries.Transition
          presentation={fade()}
          timing={linearTiming({ durationInFrames: transitionDuration })}
        />

        {/* Scene 6: Local stats */}
        <TransitionSeries.Sequence durationInFrames={scene6Duration}>
          <Scene6Stats />
        </TransitionSeries.Sequence>
      </TransitionSeries>
    </AbsoluteFill>
  );
};
