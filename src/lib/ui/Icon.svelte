<script lang="ts">
  export type IconName = "close" | "minus" | "send" | "check" | "pending" | "resize" | "arrow";
  let { name, size = 18 }: { name: IconName; size?: number } = $props();
</script>
<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  {#if name === "close"}<path d="m6 6 12 12M18 6 6 18" />
  {:else if name === "minus"}<path d="M5 12h14" />
  {:else if name === "send"}<path d="m5 12 7-7 7 7M12 5v14" />
  {:else if name === "check"}<path d="m4 12 4 4 9-9m-4 9 7-7" />
  {:else if name === "pending"}<circle cx="12" cy="12" r="8" /><path d="M12 8v4l3 2" />
  {:else if name === "resize"}<path d="m10 19 9-9m-3 9 3-3" />
  {:else}<path d="m9 5 7 7-7 7" />{/if}
</svg>
