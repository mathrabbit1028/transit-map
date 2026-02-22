/*
  Point Register Map
  - Click map to set a "selected" marker
  - Enter name and register
  - Generates ID: SSNNN (SS=session 2 digits, NNN=sequence 3 digits within session)
  - Persists to localStorage
*/

// Data source: server-backed JSON files under data/position/SS.json
// - GET  /api/position/:session -> Point[]
// - POST /api/position/:session (body: Point[]) -> { ok: true }

/** @typedef {{ id: string, session: number, seq: number, name: string, lat: number, lng: number, createdAt: string }} Point */

function pad2(n) {
  return String(n).padStart(2, '0');
}

function pad3(n) {
  return String(n).padStart(3, '0');
}

function clampInt(n, min, max) {
  const v = Number.parseInt(String(n), 10);
  if (Number.isNaN(v)) return null;
  return Math.min(max, Math.max(min, v));
}

function roundCoord(v) {
  // UI-friendly precision
  return Math.round(v * 1e6) / 1e6;
}

/** @param {number} session @returns {Promise<Point[]>} */
async function loadPoints(session) {
  const res = await fetch(`/api/position/${session}`, { cache: 'no-store' });
  if (!res.ok) throw new Error('failed to load');
  const json = await res.json();
  if (!Array.isArray(json)) return [];
  return json;
}

/** @param {number} session @param {Point[]} points */
async function savePoints(session, points) {
  const res = await fetch(`/api/position/${session}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(points),
  });
  if (!res.ok) throw new Error('failed to save');
}

/** @param {Point[]} points */
function computeNextSeq(points, session) {
  const maxSeq = points
    .filter((p) => p.session === session)
    .reduce((m, p) => Math.max(m, p.seq), 0);
  return maxSeq + 1;
}

function makeId(session, seq) {
  return `${pad2(session)}${pad3(seq)}`;
}

function escapeHtml(str) {
  return String(str)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#039;');
}

function downloadText(filename, text) {
  const blob = new Blob([text], { type: 'application/json;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

function init() {
  /** @type {HTMLInputElement} */
  const inputName = document.getElementById('inputName');
  /** @type {HTMLInputElement} */
  const inputLat = document.getElementById('inputLat');
  /** @type {HTMLInputElement} */
  const inputLng = document.getElementById('inputLng');
  /** @type {HTMLInputElement} */
  const inputSession = document.getElementById('inputSession');
  /** @type {HTMLInputElement} */
  const inputNextSeq = document.getElementById('inputNextSeq');

  const btnRegister = document.getElementById('btnRegister');
  const btnClear = document.getElementById('btnClear');
  const btnExport = document.getElementById('btnExport');
  const listEl = document.getElementById('list');
  const statsEl = document.getElementById('stats');

  const dialogExport = document.getElementById('dialogExport');
  const exportText = document.getElementById('exportText');
  const btnCopy = document.getElementById('btnCopy');

  /** @type {Point[]} */
  let points = [];
  /** @type {number} */
  let currentSession = 1;
  /** @type {boolean} */
  let isSaving = false;
  /** @type {boolean} */
  let isDirty = false;

  // Session default: 1
  inputSession.value = '1';

  // Map
  const map = L.map('map', { zoomControl: true });
  // Default center: Seoul City Hall
  map.setView([37.5665, 126.978], 13);

  L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
    maxZoom: 19,
    attribution: '&copy; OpenStreetMap contributors',
  }).addTo(map);

  // restore view after load (optional)

  /** @type {L.Marker | null} */
  let selectedMarker = null;
  /** @type {{lat:number, lng:number} | null} */
  let selectedLatLng = null;

  /** @type {Record<string, L.Marker>} */
  const pointMarkers = {};

  function setSelected(lat, lng) {
    selectedLatLng = { lat, lng };
    inputLat.value = String(roundCoord(lat));
    inputLng.value = String(roundCoord(lng));

    if (!selectedMarker) {
      selectedMarker = L.marker([lat, lng], { draggable: true }).addTo(map);
      selectedMarker.on('dragend', () => {
        const ll = selectedMarker.getLatLng();
        setSelected(ll.lat, ll.lng);
      });
    } else {
      selectedMarker.setLatLng([lat, lng]);
    }
  }

  function refreshNextSeq() {
    const session = clampInt(inputSession.value, 1, 99) ?? 1;
    const next = computeNextSeq(points, session);
    inputNextSeq.value = pad3(next);
  }

  function setDirty(v) {
    isDirty = v;
    // light indicator via button text
    btnRegister.textContent = isDirty ? '등록 (저장 대기)' : '등록';
  }

  async function autosave(reason) {
    if (isSaving) return;
    if (!isDirty) return;
    isSaving = true;
    try {
      await savePoints(currentSession, points);
      setDirty(false);
    } catch (e) {
      console.error(e);
      alert(`저장에 실패했어요. (세션 ${pad2(currentSession)})\n서버가 실행 중인지 확인해주세요.`);
    } finally {
      isSaving = false;
    }
  }

  async function loadSession(session) {
    // if there are unsaved edits, save before switching
    await autosave('before-switch');

    currentSession = session;
    try {
      points = await loadPoints(session);
      setDirty(false);
    } catch (e) {
      console.error(e);
      points = [];
      setDirty(false);
      alert(`세션 ${pad2(session)} 데이터를 읽지 못했어요. 서버가 실행 중인지 확인해주세요.`);
    }

    // adjust view to last point
    const last = points[points.length - 1];
    if (last) map.setView([last.lat, last.lng], 14);

    renderList();
  }

  function renderList() {
    statsEl.textContent = `세션 ${pad2(currentSession)} · ${points.length}개`;
    refreshNextSeq();

    // markers for registered points
    const existingIds = new Set(points.map((p) => p.id));
    // remove stale markers
    for (const id of Object.keys(pointMarkers)) {
      if (!existingIds.has(id)) {
        pointMarkers[id].remove();
        delete pointMarkers[id];
      }
    }
    // add/update markers
    for (const p of points) {
      if (!pointMarkers[p.id]) {
        const m = L.circleMarker([p.lat, p.lng], {
          radius: 7,
          color: '#6ea8ff',
          weight: 2,
          fillColor: '#6ea8ff',
          fillOpacity: 0.25,
        }).addTo(map);
        m.bindPopup(
          `<div style="font-weight:700; margin-bottom:4px;">${escapeHtml(p.name)}</div>` +
            `<div style="font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 12px; opacity: 0.9;">ID: ${p.id}</div>` +
            `<div style="font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 12px; opacity: 0.8;">${roundCoord(p.lat)}, ${roundCoord(p.lng)}</div>`
        );
        m.on('click', () => {
          setSelected(p.lat, p.lng);
          inputName.value = p.name;
        });
        pointMarkers[p.id] = m;
      } else {
        pointMarkers[p.id].setLatLng([p.lat, p.lng]);
      }
    }

    // list
    if (points.length === 0) {
      listEl.innerHTML = `<div style="padding: 12px; color: rgba(255,255,255,0.6); font-size: 12px;">아직 등록된 지점이 없어요. 지도를 클릭해서 첫 지점을 등록해보세요.</div>`;
      return;
    }

    const rows = points
      .slice()
      .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
      .map((p) => {
        const created = new Date(p.createdAt);
        const createdText = Number.isNaN(created.getTime())
          ? ''
          : created.toLocaleString('ko-KR');

        return `
          <div class="row" data-id="${p.id}">
            <div class="badge">${p.id}</div>
            <div class="row-main">
              <div class="row-title">
                <div class="name" title="${escapeHtml(p.name)}">${escapeHtml(p.name)}</div>
              </div>
              <div class="coords">${roundCoord(p.lat)}, ${roundCoord(p.lng)}${createdText ? ` · ${escapeHtml(createdText)}` : ''}</div>
              <div class="row-actions">
                <button class="btn secondary small" data-action="fly">이동</button>
                <button class="btn danger small" data-action="delete">삭제</button>
              </div>
            </div>
          </div>
        `.trim();
      })
      .join('');

    listEl.innerHTML = rows;
  }

  function registerSelected() {
    if (!selectedLatLng) {
      alert('먼저 지도에서 지점을 클릭해 선택해주세요.');
      return;
    }

    const name = inputName.value.trim();
    if (!name) {
      alert('지점 이름을 입력해주세요.');
      inputName.focus();
      return;
    }

    const session = currentSession;
    const seq = computeNextSeq(points, session);
    if (seq > 999) {
      alert('해당 등록 세션에서 999개를 초과했어요. 다른 세션 번호(앞 2자리)를 사용해주세요.');
      return;
    }

    const id = makeId(session, seq);

    /** @type {Point} */
    const point = {
      id,
      session,
      seq,
      name,
      lat: roundCoord(selectedLatLng.lat),
      lng: roundCoord(selectedLatLng.lng),
      createdAt: new Date().toISOString(),
    };

  points = [...points, point];
  setDirty(true);
  renderList();
  // save immediately on register
  void autosave('register');

    // convenience: clear name for next entry only if you want.
    inputName.select();
  }

  // map click: update selected marker
  map.on('click', (e) => {
    // Shift+click: keep existing selection marker but still update position
    setSelected(e.latlng.lat, e.latlng.lng);
  });

  // UI events
  btnRegister.addEventListener('click', registerSelected);
  inputName.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter') registerSelected();
  });

  inputSession.addEventListener('input', () => {
    // don't spam loads while typing; normalize to int if valid
    const v = clampInt(inputSession.value, 1, 99);
    if (v == null) return;
    inputSession.value = String(v);
    refreshNextSeq();
  });

  inputSession.addEventListener('change', async () => {
    const v = clampInt(inputSession.value, 1, 99) ?? 1;
    inputSession.value = String(v);
    await loadSession(v);
  });

  listEl.addEventListener('click', (e) => {
    const target = /** @type {HTMLElement} */ (e.target);
    const btn = target.closest('button');
    if (!btn) return;
    const row = target.closest('.row');
    if (!row) return;
    const id = row.getAttribute('data-id');
    if (!id) return;

    const action = btn.getAttribute('data-action');
    const p = points.find((x) => x.id === id);
    if (!p) return;

    if (action === 'fly') {
      map.flyTo([p.lat, p.lng], Math.max(map.getZoom(), 15), { duration: 0.6 });
      if (pointMarkers[p.id]) pointMarkers[p.id].openPopup();
      setSelected(p.lat, p.lng);
      inputName.value = p.name;
      inputName.focus();
      return;
    }

    if (action === 'delete') {
      if (!confirm(`${p.name} (${p.id}) 을(를) 삭제할까요?`)) return;
      points = points.filter((x) => x.id !== id);
      setDirty(true);
      renderList();
      void autosave('delete');
    }
  });

  btnClear.addEventListener('click', () => {
    if (points.length === 0) return;
    if (!confirm('등록된 지점을 전부 삭제할까요?')) return;
    points = [];
    setDirty(true);
    renderList();
    void autosave('clear');
  });

  btnExport.addEventListener('click', () => {
    exportText.value = JSON.stringify(points, null, 2);
    dialogExport.showModal();
  });

  btnCopy.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(exportText.value);
    } catch {
      // clipboard 실패 시 다운로드로 대체
      const filename = `points-${new Date().toISOString().slice(0, 10)}.json`;
      downloadText(filename, exportText.value);
    }
  });

  // initial selection
  setSelected(37.5665, 126.978);

  // initial load
  void loadSession(1);
}

window.addEventListener('DOMContentLoaded', () => {
  // Leaflet script is deferred; ensure it's available
  if (typeof L === 'undefined') {
    document.body.innerHTML =
      '<div style="padding:16px; font-family: system-ui;">Leaflet 로딩에 실패했어요. 인터넷 연결을 확인해주세요.</div>';
    return;
  }
  init();
});
