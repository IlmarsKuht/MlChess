import { HashRouter, Route, Routes } from "react-router-dom";

import { AppLayout } from "../layout/AppLayout";
import { useDebugBootstrap } from "../../features/debug/useDebugBootstrap";
import { EngineDetailPage, EnginesPage } from "../../features/engines/route";
import { EventsPage } from "../../features/events/route";
import { HumanGamePage } from "../../features/human-game/route";
import { LiveDuelPage } from "../../features/live-duel/route";
import { OverviewPage } from "../../features/overview/route";
import { ReplayPage } from "../../features/replay/route";
import { TournamentsPage } from "../../features/tournaments/route";
import { WatchPage } from "../../features/watch/route";

function DebugBootstrapper() {
  useDebugBootstrap();
  return null;
}

export function AppRoutes() {
  return (
    <HashRouter>
      <DebugBootstrapper />
      <Routes>
        <Route element={<AppLayout />}>
          <Route index element={<OverviewPage />} />
          <Route path="/setup" element={<EnginesPage />} />
          <Route path="/engine/:engineId" element={<EngineDetailPage />} />
          <Route path="/live-duel" element={<LiveDuelPage />} />
          <Route path="/play-engine" element={<HumanGamePage />} />
          <Route path="/events" element={<EventsPage />} />
          <Route path="/tournaments" element={<TournamentsPage />} />
          <Route path="/replay" element={<ReplayPage />} />
        </Route>
        <Route path="/watch/:matchId" element={<WatchPage />} />
      </Routes>
    </HashRouter>
  );
}
