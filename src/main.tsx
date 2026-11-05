import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import PetApp from "./pet/PetApp";
import PanelApp from "./panel/PanelApp";
import ToastApp from "./toast/ToastApp";
import MenuApp from "./menu/MenuApp";
import MiniApp from "./mini/MiniApp";
import CaptionsApp from "./captions/CaptionsApp";
import "./styles.css";
import { setupSound } from "./ui/sound";
import { setupTooltips } from "./ui/tooltip";

// Um único bundle para as duas janelas: o rótulo da janela decide o que renderizar.
const label = getCurrentWindow().label;
setupSound();
setupTooltips();
const App = label === "panel" ? PanelApp : label === "toast" ? ToastApp : label === "menu" ? MenuApp : label === "mini" ? MiniApp : label === "captions" ? CaptionsApp : PetApp;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
