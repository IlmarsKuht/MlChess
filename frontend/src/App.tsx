import { FlashProvider } from "./app/providers/FlashProvider";
import { QueryProvider } from "./app/providers/QueryProvider";
import { AppRoutes } from "./app/routes";

export default function App() {
  return (
    <QueryProvider>
      <FlashProvider>
        <AppRoutes />
      </FlashProvider>
    </QueryProvider>
  );
}
