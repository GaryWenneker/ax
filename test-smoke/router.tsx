import { createBrowserRouter } from "react-router-dom";
import { Dashboard } from "./Dashboard";

export const router = createBrowserRouter([
  { path: "/dashboard", element: <Dashboard /> },
]);