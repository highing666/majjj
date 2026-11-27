import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { clearInviteCodeFromLocation } from "./lib/remote/inviteUrl";

clearInviteCodeFromLocation();

const target = document.getElementById("app");
if (!target) {
  throw new Error("Missing #app mount element");
}

mount(App, { target });
