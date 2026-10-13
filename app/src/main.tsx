import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Welcome from "./Welcome";

const view = new URLSearchParams(window.location.search).get("view");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {view === "welcome" ? <Welcome /> : <App />}
  </React.StrictMode>,
);
