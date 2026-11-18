<script>
  let {
    checked = false,
    onchange = () => {},
    disabled = false,
    activeColor = 'bg-emerald-500',
    ariaLabel = 'Переключатель',
  } = $props();

  function toggle() {
    if (disabled) return;
    onchange(!checked);
  }
</script>

<button
  type="button"
  role="switch"
  aria-label={ariaLabel}
  aria-checked={checked}
  onclick={toggle}
  {disabled}
  class="toggle-switch relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors cursor-pointer focus:outline-none {checked
    ? (activeColor || 'bg-emerald-500') + ' is-checked'
    : 'bg-slate-700 is-unchecked'} {disabled ? 'opacity-50 cursor-not-allowed' : ''}"
>
  <span
    class="toggle-thumb inline-block h-5 w-5 transform rounded-full bg-white shadow-md transition-transform pointer-events-none {checked
      ? 'translate-x-5.5'
      : 'translate-x-0.5'}"
  ></span>
</button>
