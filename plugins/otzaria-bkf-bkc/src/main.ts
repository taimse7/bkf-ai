import "./styles.css";

interface Connection {
  port: number;
  token: string;
}

interface Repository {
  id: string;
  displayName: string;
  connected: boolean;
  documentCount: number;
}

interface DocumentItem {
  id: string;
  name: string;
  repositoryName: string;
  format: string;
  supportStatus: string;
  cachePdfPath: string | null;
}

interface SearchHit {
  documentId: string;
  documentName: string;
  repositoryName: string;
  pageIndex: number;
  snippet: string;
}

const root = document.querySelector<HTMLDivElement>("#app")!;
let connection: Connection = { port: 47831, token: "" };
let repositories: Repository[] = [];
let selectedRepositoryIds: string[] = [];
let activePdfUrl = "";

function apiUrl(path: string) {
  return `http://127.0.0.1:${connection.port}${path}`;
}

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(apiUrl(path), {
    ...init,
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${connection.token}`,
      ...(init?.headers ?? {})
    }
  });
  if (!response.ok) {
    throw new Error(`${response.status} ${await response.text()}`);
  }
  return response.json() as Promise<T>;
}

async function storageGet(key: string): Promise<unknown> {
  if (!window.Otzaria) return localStorage.getItem(key);
  const response = await window.Otzaria.call("storage.get", { key });
  return response.success ? response.data : null;
}

async function storageSet(key: string, value: unknown) {
  if (!window.Otzaria) {
    localStorage.setItem(key, JSON.stringify(value));
    return;
  }
  await window.Otzaria.call("storage.set", { key, value });
}

async function loadConnection() {
  const stored = await storageGet("bkf-ai-connection");
  if (typeof stored === "string") {
    try {
      connection = JSON.parse(stored) as Connection;
    } catch {
      // keep defaults
    }
  } else if (stored && typeof stored === "object") {
    connection = stored as Connection;
  }
}

async function loadTheme() {
  if (!window.Otzaria) return;
  const response = await window.Otzaria.call("app.getTheme");
  if (!response.success || !response.data) return;
  applyTheme(response.data as any);
}

function applyTheme(theme: any) {
  const colors = theme.colorScheme ?? {};
  const style = document.documentElement.style;
  style.setProperty("--primary", colors.primary ?? "#2e6e9e");
  style.setProperty("--on-primary", colors.onPrimary ?? "#fff");
  style.setProperty("--surface", colors.surface ?? "#fff");
  style.setProperty("--text", colors.onSurface ?? "#222");
  style.setProperty("--panel", colors.surfaceContainerHighest ?? "#f0f2f4");
  style.setProperty("--outline", colors.outline ?? "#ccd3d8");
  document.documentElement.dataset.mode = theme.mode ?? "light";
}

async function connect() {
  try {
    const health = await fetch(apiUrl("/api/v1/health")).then((response) => response.json());
    if (!health.ok) throw new Error("המנוע אינו זמין");
    repositories = await api<Repository[]>("/api/v1/repositories");
    selectedRepositoryIds = repositories.map((item) => item.id);
    renderLibrary();
  } catch (error) {
    renderSetup(String(error));
  }
}

function renderSetup(error = "") {
  root.innerHTML = `
    <main class="setup">
      <h1>חיבור למנוע BKF AI</h1>
      <p>הפעל את אפליקציית BKF AI במחשב והעתק ממנה את ה־Token המקומי.</p>
      ${error ? `<div class="error">${escapeHtml(error)}</div>` : ""}
      <label>פורט
        <input id="port" type="number" value="${connection.port}" />
      </label>
      <label>Token
        <input id="token" value="${escapeHtml(connection.token)}" dir="ltr" />
      </label>
      <button id="save">שמור והתחבר</button>
    </main>
  `;
  root.querySelector<HTMLButtonElement>("#save")!.onclick = async () => {
    connection = {
      port: Number(root.querySelector<HTMLInputElement>("#port")!.value) || 47831,
      token: root.querySelector<HTMLInputElement>("#token")!.value.trim()
    };
    await storageSet("bkf-ai-connection", connection);
    await connect();
  };
}

function renderLibrary() {
  root.innerHTML = `
    <main class="shell">
      <aside class="library">
        <header>
          <strong>ספריית BKF/BKC</strong>
          <button id="settings">חיבור</button>
        </header>
        <select id="repository">
          <option value="">כל המאגרים</option>
          ${repositories.map((repository) => `
            <option value="${repository.id}">
              ${escapeHtml(repository.displayName)} (${repository.documentCount})
            </option>
          `).join("")}
        </select>
        <input id="name-search" type="search" placeholder="חיפוש לפי שם" />
        <div class="search-row">
          <input id="text-search" type="search" placeholder="חיפוש בטקסט" />
          <button id="run-search">חפש</button>
        </div>
        <div id="results" class="results"></div>
      </aside>
      <section class="viewer">
        <header>
          <span id="viewer-title">לא נבחר מסמך</span>
          <button id="fullscreen">מסך מלא</button>
        </header>
        <iframe id="pdf-viewer" title="תצוגת PDF" hidden></iframe>
        <div id="empty" class="empty">בחר מסמך או תוצאת חיפוש.</div>
      </section>
    </main>
  `;

  root.querySelector<HTMLButtonElement>("#settings")!.onclick = () => renderSetup();
  root.querySelector<HTMLButtonElement>("#fullscreen")!.onclick = () => {
    void root.querySelector<HTMLElement>(".viewer")?.requestFullscreen();
  };
  root.querySelector<HTMLSelectElement>("#repository")!.onchange = (event) => {
    const value = (event.target as HTMLSelectElement).value;
    selectedRepositoryIds = value ? [value] : repositories.map((item) => item.id);
    void loadDocuments();
  };
  root.querySelector<HTMLInputElement>("#name-search")!.oninput = debounce(() => {
    void loadDocuments();
  }, 180);
  root.querySelector<HTMLButtonElement>("#run-search")!.onclick = () => void runTextSearch();
  root.querySelector<HTMLInputElement>("#text-search")!.onkeydown = (event) => {
    if (event.key === "Enter") void runTextSearch();
  };
  void loadDocuments();
}

async function loadDocuments() {
  const query = root.querySelector<HTMLInputElement>("#name-search")?.value ?? "";
  const params = new URLSearchParams({
    repositoryIds: selectedRepositoryIds.join(","),
    query,
    offset: "0",
    limit: "200"
  });
  try {
    const page = await api<{ items: DocumentItem[]; total: number }>(
      `/api/v1/documents?${params}`
    );
    renderDocuments(page.items, page.total);
  } catch (error) {
    renderResultError(String(error));
  }
}

function renderDocuments(items: DocumentItem[], total: number) {
  const results = root.querySelector<HTMLDivElement>("#results")!;
  results.innerHTML = `
    <small>${total.toLocaleString("he-IL")} מסמכים</small>
    ${items.map((item) => `
      <button class="document" data-id="${item.id}">
        <span class="badge">${item.format}</span>
        <strong>${escapeHtml(item.name)}</strong>
        <small>${escapeHtml(item.repositoryName)}</small>
      </button>
    `).join("")}
  `;
  results.querySelectorAll<HTMLButtonElement>(".document").forEach((button) => {
    button.onclick = () => void openDocument(button.dataset.id!);
  });
}

async function runTextSearch() {
  const query = root.querySelector<HTMLInputElement>("#text-search")!.value.trim();
  if (!query) return;
  try {
    const hits = await api<SearchHit[]>("/api/v1/search", {
      method: "POST",
      body: JSON.stringify({
        query,
        repositoryIds: selectedRepositoryIds,
        limit: 100
      })
    });
    const results = root.querySelector<HTMLDivElement>("#results")!;
    results.innerHTML = hits.map((hit) => `
      <button class="hit" data-id="${hit.documentId}" data-page="${hit.pageIndex}">
        <strong>${escapeHtml(hit.documentName)}</strong>
        <span>עמוד ${hit.pageIndex + 1}</span>
        <p>${escapeHtml(hit.snippet)}</p>
        <small>${escapeHtml(hit.repositoryName)}</small>
      </button>
    `).join("") || `<div class="empty">לא נמצאו תוצאות.</div>`;
    results.querySelectorAll<HTMLButtonElement>(".hit").forEach((button) => {
      button.onclick = () => void openDocument(button.dataset.id!, Number(button.dataset.page ?? 0));
    });
  } catch (error) {
    renderResultError(String(error));
  }
}

async function openDocument(documentId: string, page = 0) {
  try {
    const prepared = await api<{
      kind: string;
      title: string;
      message: string | null;
      pdfUrl: string | null;
    }>(`/api/v1/documents/${documentId}/prepare`, { method: "POST" });

    root.querySelector<HTMLElement>("#viewer-title")!.textContent = prepared.title;
    const frame = root.querySelector<HTMLIFrameElement>("#pdf-viewer")!;
    const empty = root.querySelector<HTMLElement>("#empty")!;

    if (prepared.kind !== "pdf" || !prepared.pdfUrl) {
      frame.hidden = true;
      empty.hidden = false;
      empty.textContent = prepared.message ?? "המסמך אינו זמין לתצוגה.";
      return;
    }

    activePdfUrl = `${apiUrl(prepared.pdfUrl)}?token=${encodeURIComponent(connection.token)}#page=${page + 1}`;
    frame.src = activePdfUrl;
    frame.hidden = false;
    empty.hidden = true;
  } catch (error) {
    renderResultError(String(error));
  }
}

function renderResultError(error: string) {
  root.querySelector<HTMLDivElement>("#results")!.innerHTML =
    `<div class="error">${escapeHtml(error)}</div>`;
}

function debounce(callback: () => void, wait: number) {
  let timer = 0;
  return () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(callback, wait);
  };
}

function escapeHtml(value: string) {
  return value.replace(/[&<>"']/g, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#039;"
  })[character]!);
}

async function boot() {
  await loadConnection();
  await loadTheme();
  if (window.Otzaria) {
    window.Otzaria.on("theme.changed", applyTheme);
  }
  if (!connection.token) {
    renderSetup();
  } else {
    await connect();
  }
}

void boot();
