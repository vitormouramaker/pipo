import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import PetApp from "./pet/PetApp";
import PanelApp from "./panel/PanelApp";
import "./styles.css";

// Um único bundle para as duas janelas: o rótulo da janela decide o que renderizar.
const isPanel = getCurrentWindow().label === "panel";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isPanel ? <PanelApp /> : <PetApp />}</React.StrictMode>,
);
