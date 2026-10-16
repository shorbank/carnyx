<script lang="ts">
  import { onMount } from "svelte";
  import type { ChatWorkspace } from "$lib/controllers/chat-workspace";
  import type { WindowApi } from "$lib/adapters/window-api";
  import AppHeader from "$lib/components/shell/AppHeader.svelte";
  import ChatTabs from "$lib/components/shell/ChatTabs.svelte";
  import ConversationView from "$lib/components/chat/ConversationView.svelte";
  import ConnectionView from "$lib/components/connection/ConnectionView.svelte";
  import SavedPeerActions from "$lib/components/connection/SavedPeerActions.svelte";
  import { peerError } from "$lib/presentation/peer-errors";
  import ResizeHandle from "$lib/components/shell/ResizeHandle.svelte";
  import StatusNotice from "$lib/components/ui/StatusNotice.svelte";

  let { workspace, windowApi }: { workspace: ChatWorkspace; windowApi: WindowApi } = $props();
  const workspaceState = $derived(workspace.state);
  const setupForm = $derived(workspace.setup.state);
  let windowError = $state("");
  async function windowAction(action: () => Promise<void>) {
    windowError = "";
    try { await action(); } catch { windowError = "Die Fensteraktion konnte nicht ausgeführt werden."; }
  }
  onMount(() => {
    void workspace.start();
    const onfocus = () => { void workspace.refresh(); };
    window.addEventListener("focus", onfocus);
    return () => { window.removeEventListener("focus", onfocus); workspace.dispose(); };
  });
</script>
<section class="app" aria-label="Carnyx">
  <AppHeader ondrag={() => { void windowAction(windowApi.drag); }} onminimize={() => { void windowAction(windowApi.minimize); }} onclose={() => { void windowAction(windowApi.close); }} />
  <ChatTabs selected={$workspaceState.view} connection={$workspaceState.connection.connection} onselect={workspace.select} />
  {#if windowError}<div class="window-error"><StatusNotice text={windowError} error /></div>{/if}
  <div class="content">
    <div class="panel" role="tabpanel" id="panel-local" aria-labelledby="tab-local" hidden={$workspaceState.view !== "local"}><ConversationView conversation={$workspaceState.local} visible={$workspaceState.view === "local"} /></div>
    <div class="panel" role="tabpanel" id="panel-peer" aria-labelledby="tab-peer" hidden={$workspaceState.view !== "peer"}>
      {#if $workspaceState.peer}
        <div class="peer-conversation">
          {#if $workspaceState.connection.error || $workspaceState.connection.setupError || $workspaceState.connection.connection.error}
            <div class="peer-notice"><StatusNotice error text={$workspaceState.connection.error ?? $workspaceState.connection.setupError ?? peerError($workspaceState.connection.connection.error)}>{#if $workspaceState.connection.setupError}<button type="button" disabled={$workspaceState.connection.busy || $setupForm.loading} onclick={() => { void workspace.setup.load(true); }}>Erneut laden</button>{/if}</StatusNotice></div>
          {/if}
          {#if !$workspaceState.peerCanSend && $workspaceState.connection.connection.phase === "disconnected" && $workspaceState.connection.setup?.selectedPeer}
            <SavedPeerActions compact peer={$workspaceState.connection.setup.selectedPeer} roleLabel={$setupForm.roleLabel} canReconnect={$setupForm.canReconnect} hint={$setupForm.reconnectHint} busy={$workspaceState.connection.busy || $setupForm.loading} onreconnect={() => { void workspace.setup.reconnect(); }} onedit={() => { workspace.select("connection"); workspace.setup.edit(); }} />
          {/if}
          <div class="peer-history">{#key $workspaceState.peer.id}<ConversationView conversation={$workspaceState.peer} canSend={$workspaceState.peerCanSend} visible={$workspaceState.view === "peer"} />{/key}</div>
        </div>
      {:else if $workspaceState.connection.setupStatus === "idle" || $workspaceState.connection.setupStatus === "loading"}
        <div class="peer-notice"><StatusNotice text="Gespeicherte Verbindung wird geladen …" /></div>
      {:else if $workspaceState.connection.setupStatus === "error"}
        <div class="peer-notice"><StatusNotice error text={$workspaceState.connection.setupError ?? "Die gespeicherte Verbindung konnte nicht geladen werden."}><button type="button" onclick={() => { void workspace.setup.load(true); }}>Erneut versuchen</button></StatusNotice></div>
      {:else}
        <div class="empty"><span class="empty-mark" aria-hidden="true">↔</span><h1>Zusammen schreiben</h1><p>Richte eine direkte Verbindung zu deinem Gegenüber ein.</p><button type="button" class="primary" onclick={() => workspace.select("connection")}>Verbindung einrichten</button></div>
      {/if}
    </div>
    <div class="panel" role="tabpanel" id="panel-connection" aria-labelledby="tab-connection" hidden={$workspaceState.view !== "connection"}><ConnectionView connection={$workspaceState.connection} controller={workspace.setup} /></div>
  </div>
  <ResizeHandle onresize={() => { void windowAction(windowApi.resize); }} />
</section>
<style>
  .app { position: relative; display: flex; flex-direction: column; height: 100%; width: 100%; background: var(--background); overflow: hidden; }
  .content { flex: 1; min-height: 0; }
  .panel { height: 100%; min-height: 0; }
  .peer-conversation { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .peer-history { flex: 1; min-height: 0; }
  .peer-notice { padding: 10px 16px; }
  .window-error { padding: 8px 12px; }
  .empty { height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; text-align: center; padding: 24px; }
  .empty h1 { font-size: 17px; font-weight: 600; margin: 0; }
  .empty p { color: var(--muted); max-width: 240px; }
  .empty-mark { display: grid; place-items: center; width: 44px; height: 44px; border-radius: 10px; background: var(--surface); color: var(--accent); font-size: 24px; }
</style>
