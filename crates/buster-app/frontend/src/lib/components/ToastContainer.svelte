<script>
  import { toast } from '../stores/toast.svelte.js';
  import { CheckCircle2, AlertCircle, AlertTriangle, Info, X } from 'lucide-svelte';
  import { fly, fade } from 'svelte/transition';
</script>

<div class="fixed top-14 right-5 z-50 flex flex-col gap-2.5 max-w-sm w-full pointer-events-none">
  {#each toast.toasts as t (t.id)}
    <div
      in:fly={{ y: -15, duration: 250 }}
      out:fade={{ duration: 150 }}
      class="toast-card pointer-events-auto backdrop-blur-xl bg-slate-900/95 border border-slate-700 shadow-2xl rounded-2xl p-3.5 flex items-start gap-3 text-white transition-all transform-gpu"
    >
      <!-- Icon based on type -->
      <div class="shrink-0 mt-0.5">
        {#if t.type === 'success'}
          <div class="w-7 h-7 rounded-xl bg-emerald-500/20 border border-emerald-500/30 flex items-center justify-center text-emerald-400">
            <CheckCircle2 class="w-4 h-4" />
          </div>
        {:else if t.type === 'error'}
          <div class="w-7 h-7 rounded-xl bg-rose-500/20 border border-rose-500/30 flex items-center justify-center text-rose-400">
            <AlertCircle class="w-4 h-4" />
          </div>
        {:else if t.type === 'warning'}
          <div class="w-7 h-7 rounded-xl bg-amber-500/20 border border-amber-500/30 flex items-center justify-center text-amber-400">
            <AlertTriangle class="w-4 h-4" />
          </div>
        {:else}
          <div class="w-7 h-7 rounded-xl bg-accent/20 border border-accent/30 flex items-center justify-center text-accent">
            <Info class="w-4 h-4" />
          </div>
        {/if}
      </div>

      <!-- Content -->
      <div class="flex-1 min-w-0 pr-1">
        {#if t.title}
          <div class="toast-title font-unbounded font-semibold text-xs tracking-tight text-white mb-0.5">
            {t.title}
          </div>
        {/if}
        <div class="toast-msg text-[11.5px] text-white/90 leading-snug break-words">
          {t.message}
        </div>
      </div>

      <!-- Dismiss Button -->
      <button
        onclick={() => toast.remove(t.id)}
        class="toast-close shrink-0 p-1 rounded-lg text-white/40 hover:text-white hover:bg-white/10 transition-colors"
        aria-label="Закрыть уведомление"
      >
        <X class="w-3.5 h-3.5" />
      </button>
    </div>
  {/each}
</div>
