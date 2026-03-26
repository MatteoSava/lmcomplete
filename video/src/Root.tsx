import { Composition, Folder } from "remotion";
import { LmcDemo } from "./LmcDemo";

export const RemotionRoot: React.FC = () => {
  return (
    <>
      <Folder name="Launch">
        <Composition
          id="LmcDemo"
          component={LmcDemo}
          durationInFrames={1545}
          fps={30}
          width={1920}
          height={1080}
          defaultProps={{}}
        />
      </Folder>
    </>
  );
};
