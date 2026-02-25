const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

function colorClass(color) {
  switch (color) {
    case 'Green': return 'green';
    case 'Yellow': return 'yellow';
    case 'Red': return 'red';
    case 'RedBlink': return 'red-blink';
    default: return 'gray';
  }
}

function updateBar(prefix, bar) {
  const fill = document.getElementById(`${prefix}-fill`);
  const pct = document.getElementById(`${prefix}-pct`);
  const reset = document.getElementById(`${prefix}-reset`);
  const proj = document.getElementById(`${prefix}-projected`);
  const gap = document.getElementById(`${prefix}-gap`);

  if (!bar) {
    fill.style.width = '0%';
    fill.className = 'bar-fill gray';
    pct.textContent = '--%';
    reset.textContent = 'no data';
    proj.textContent = '';
    gap.textContent = '';
    return;
  }

  const utilization = Math.min(bar.utilization, 100);
  fill.style.width = `${utilization}%`;
  fill.className = `bar-fill ${colorClass(bar.color)}`;
  pct.textContent = `${Math.round(bar.utilization)}%`;
  reset.textContent = bar.reset_display;
  proj.textContent = `→ ${Math.round(bar.projected)}%`;
  gap.textContent = bar.gap_display || '';

  // Projected marker
  const container = fill.parentElement;
  let marker = container.querySelector('.projected-marker');
  if (bar.projected > bar.utilization + 5 && bar.projected <= 120) {
    if (!marker) {
      marker = document.createElement('div');
      marker.className = 'projected-marker';
      container.appendChild(marker);
    }
    marker.style.left = `${Math.min(bar.projected, 100)}%`;
    marker.title = `Projected: ${Math.round(bar.projected)}%`;
  } else if (marker) {
    marker.remove();
  }
}

function updateUI(state) {
  if (!state) return;

  updateBar('session', state.session);
  updateBar('weekly', state.weekly);

  const errorMsg = document.getElementById('error-msg');
  errorMsg.textContent = state.error || '';
}

async function loadData() {
  try {
    const state = await invoke('get_usage');
    if (state) {
      updateUI(state);
      return true;
    }
  } catch (_) {}
  return false;
}

// Close button handler
document.getElementById('close-btn').addEventListener('click', () => {
  invoke('hide_popup').catch(() => {});
});

// Escape key to close
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    invoke('hide_popup').catch(() => {});
  }
});

async function init() {
  // Listen for live updates from backend
  await listen('usage-updated', (event) => {
    updateUI(event.payload);
  });

  // Load current data with retries
  const loaded = await loadData();
  if (!loaded) {
    setTimeout(loadData, 1000);
    setTimeout(loadData, 3000);
  }
}

init();
