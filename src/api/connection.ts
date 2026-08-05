import { invoke } from "@tauri-apps/api/core";
import { resetConnectionCache } from "./client";

export interface ServerConfigInfo {
  url: string | null;
  has_token: boolean;
}

export function getServerConfig(): Promise<ServerConfigInfo> {
  return invoke("get_server_config");
}

export function getServerToken(): Promise<string | null> {
  return invoke("get_server_token");
}

export async function setServerConfig(url: string, token: string): Promise<void> {
  await invoke("set_server_config", { url, token });
  resetConnectionCache();
}

export async function clearServerConfig(): Promise<void> {
  await invoke("clear_server_config");
  resetConnectionCache();
}
