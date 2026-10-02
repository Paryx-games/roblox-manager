(() => {
  const layer = document.querySelector('.page-transition-layer');
  const accountsView = document.createElement('section');
  accountsView.className = 'preview-workspace-view';
  accountsView.dataset.workspace = 'Accounts';
  accountsView.append(...Array.from(layer.children));
  layer.append(accountsView);
  const viewNames = ['Accounts', 'Instances', 'Groups', 'Private Servers', 'Presets', 'Settings'];
  const views = new Map([['Accounts', accountsView]]);
  const instances = new Map([
    [4100, { account: 'Alex Builder', userId: '123456789', placeId: '123456789' }],
    [4200, { account: 'Studio Tester', userId: '345678901', placeId: '123456789' }],
  ]);
  const bookmarks = new Map([['Development server', { placeId: '123456789', url: 'Demo bookmark' }]]);
  const memberships = new Set(['alex']);
  const settings = new Map([['Multi-instance', true], ['Auto-tile windows', false], ['Confirm before closing clients', true]]);
  let currentWorkspace = 'Accounts';
  const icon = name => `<img class="account-icon" src="${name}.svg" alt="">`;
  const escapeText = value => String(value).replace(/[&<>"']/g, character => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character]);
  const actionButton = (label, action, iconName) => `<button class="account-button" data-preview-action="${action}">${iconName ? icon(iconName) : ''}${label}</button>`;
  const header = name => `<div class="header-row"><h1 class="header-title">${name}</h1></div>`;
  const nativeDescription = action => {
    showDialog(action);
    document.getElementById('dialog-description').textContent = 'This control is available in the installed Windows app. The website preview uses synthetic data and does not connect to Roblox or control real clients.';
  };

  function confirmPreviewAction(title, description, onConfirm) {
    showDialog(title);
    document.getElementById('dialog-description').textContent = description;
    const confirmation = document.createElement('button');
    confirmation.className = 'account-button primary preview-confirmation';
    confirmation.textContent = 'Confirm';
    closeButton.textContent = 'Cancel';
    closeButton.before(confirmation);
    confirmation.addEventListener('click', () => { onConfirm(); confirmation.remove(); closeDialog(); });
    confirmation.focus();
  }

  const originalShowDialog = showDialog;
  showDialog = action => { document.querySelector('.preview-confirmation')?.remove(); originalShowDialog(action); };
  closeButton.addEventListener('click', () => document.querySelector('.preview-confirmation')?.remove());
  document.addEventListener('keydown', event => { if (event.key === 'Escape') document.querySelector('.preview-confirmation')?.remove(); });

  function createView(name, markup) {
    const view = document.createElement('section');
    view.className = 'preview-workspace-view';
    view.dataset.workspace = name;
    view.hidden = true;
    view.innerHTML = header(name) + markup;
    layer.append(view);
    views.set(name, view);
    return view;
  }

  const instancesView = createView('Instances', `<main class="instances-page assets-page"><div class="assets-main"><div class="assets-toolbar instances-toolbar"><span class="instances-count" id="preview-instance-count" role="status"></span><div class="instances-toolbar-actions">${actionButton('Refresh', 'refresh-instances', 'refresh')}${actionButton('Arrange windows', 'arrange-windows')}${actionButton('Kill all Roblox', 'close-all')}</div></div><div class="assets-toolbar"><span class="instances-picker-label">Join server as</span><select class="preview-select" aria-label="Join server as"><option>Alex Builder</option><option>Studio Tester</option><option>QA Player</option></select><span class="instances-description">Choose accounts, then use Join server on a matched client.</span></div><div class="assets-panel"><table class="assets-table"><caption class="instances-caption">Running clients</caption><thead><tr><th>Account</th><th>PID</th><th>Place ID</th><th>Match</th><th>Launched</th><th>Actions</th></tr></thead><tbody id="preview-instance-rows"></tbody></table></div><p class="preview-note" id="instance-notice" role="status">Synthetic clients for the workspace preview</p></div></main>`);

  function renderInstances() {
    document.getElementById('preview-instance-count').textContent = `${instances.size} Roblox client${instances.size === 1 ? '' : 's'} running`;
    document.querySelector('.titlebar-runtime-status').textContent = `${instances.size} Roblox running`;
    document.getElementById('preview-instance-rows').innerHTML = instances.size ? Array.from(instances, ([processId, instance]) => `<tr><td><strong>${instance.account}</strong><small>${instance.userId}</small></td><td>${processId}</td><td>${instance.placeId}</td><td><span class="instances-attribution" data-attribution="exact">Exact</span></td><td>Preview session</td><td><div class="assets-row-actions">${actionButton('Focus', 'focus-' + processId)}${actionButton('Join server', 'join-' + processId, 'launch')}${actionButton('Kill', 'close-' + processId)}</div></td></tr>`).join('') : '<tr><td colspan="6">No demo clients running</td></tr>';
    instancesView.querySelector('[data-preview-action="close-all"]').disabled = !instances.size;
    instancesView.querySelector('[data-preview-action="arrange-windows"]').disabled = !instances.size;
  }

  const presetsView = createView('Presets', `<main class="presets-page"><section class="preset-card"><div class="preset-card-head"><h2>New preset</h2>${actionButton('Open folder', 'open-folder')}</div><form id="preview-preset-form"><div class="preset-form"><label><span>Name</span><input name="name" required maxlength="80" placeholder="Multiplayer test"></label><label><span>Place ID</span><input name="placeId" required inputmode="numeric" pattern="[0-9]+" placeholder="123456789"></label><label><span>Job ID (optional)</span><input name="jobId" placeholder="Specific server GUID"></label><label><span>Data (optional)</span><input name="launchData" placeholder="Extra launch query data"></label></div><div class="preset-form-actions"><button class="account-button primary" type="submit">${icon('add')}Add preset</button></div></form></section><section class="preset-card"><h2>Saved presets</h2><div class="preset-toolbar"><label class="preset-search">${icon('search')}<input id="preview-preset-search" aria-label="Search presets" placeholder="Search presets"></label></div><div class="preset-rows" id="preview-preset-rows"></div><p class="preview-note">Presets stay in this preview session only.</p></section></main>`);

  function renderPresets() {
    const query = document.getElementById('preview-preset-search').value.toLowerCase();
    const matches = Array.from(previewPresets).filter(([name]) => name.toLowerCase().includes(query));
    document.getElementById('preview-preset-rows').innerHTML = matches.length ? matches.map(([name, preset]) => `<div class="preset-row"><span class="preset-thumb">${icon('game')}</span><div class="preset-info"><strong>${escapeText(name)}</strong><div class="preset-details">Place ID: ${escapeText(preset.placeId)}</div></div><div class="preset-actions"><button class="account-button primary" data-preset-use="${escapeText(name)}">Use preset</button><button class="icon-button" data-preset-delete="${escapeText(name)}" aria-label="Delete ${escapeText(name)}">${icon('close')}</button></div></div>`).join('') : '<div class="account-empty-inline"><strong>No saved presets</strong><span>Create one above or save launch details from Accounts.</span></div>';
  }

  presetsView.querySelector('form').addEventListener('submit', event => {
    event.preventDefault();
    const form = event.currentTarget;
    const values = new FormData(form);
    const name = String(values.get('name')).trim();
    if (!name || !/^\d+$/.test(String(values.get('placeId')))) return;
    previewPresets.set(name, { placeId: String(values.get('placeId')), jobId: String(values.get('jobId')), launchData: String(values.get('launchData')) });
    form.reset(); renderPresets();
  });
  document.getElementById('preview-preset-search').addEventListener('input', renderPresets);

  const groupsView = createView('Groups', `<main class="groups-page"><aside class="groups-rail"><section class="groups-card"><h2>Find a group</h2><p class="groups-subtitle">Search the synthetic group in this preview.</p><label class="groups-input-wrap">${icon('search')}<input id="preview-group-search" aria-label="Search by group name" placeholder="Search by group name"></label><button class="groups-button primary" data-preview-action="search-groups">${icon('search')}Search groups</button></section><section class="groups-card"><div class="groups-card-head"><h2>Selected accounts</h2></div><select class="preview-select" id="preview-group-account" aria-label="Select account">${Object.entries(accounts).map(([key, account]) => `<option value="${key}">${account.name}</option>`).join('')}</select><div class="groups-action-row"><button class="groups-button primary" data-preview-action="join-group">Join selected</button><button class="groups-button" data-preview-action="leave-group">Leave selected</button></div></section></aside><section class="groups-detail"><section class="groups-card"><h2>Development Studio</h2><div class="groups-about"><p class="groups-about-description">A synthetic group for exploring membership controls.</p><div class="groups-about-meta"><span>Group ID: 1234567</span><span>Owner: Alex Builder</span></div></div></section><section class="groups-card"><h2>Membership</h2><div class="groups-membership-list" id="preview-group-memberships"></div><p id="group-notice" class="groups-notice" role="status"></p></section></section></main>`);
  function renderMemberships() {
    document.getElementById('preview-group-memberships').innerHTML = Object.entries(accounts).map(([key, account]) => `<div class="groups-membership"><strong>${account.name}</strong><span class="groups-data">${memberships.has(key) ? 'Member' : 'Not a member'}</span></div>`).join('');
    const selectedAccount = document.getElementById('preview-group-account').value;
    groupsView.querySelector('[data-preview-action="join-group"]').disabled = memberships.has(selectedAccount);
    groupsView.querySelector('[data-preview-action="leave-group"]').disabled = !memberships.has(selectedAccount);
  }
  document.getElementById('preview-group-account').addEventListener('change', renderMemberships);

  const serversView = createView('Private Servers', `<main class="private-servers-page"><section class="private-server-card"><h2>Add Private Server</h2><form id="preview-server-form"><div class="private-server-fields"><label>Name<input name="name" required maxlength="80" placeholder="Development server"></label><label>URL<input name="url" required placeholder="Demo private server link"></label></div><div class="private-server-form-actions"><button class="account-button primary" type="submit">${icon('add')}Add Server</button></div></form></section><section class="private-server-card"><h2>Saved Private Servers</h2><div id="preview-server-rows"></div><p class="preview-note">Bookmarks are synthetic and stay in this preview session.</p></section></main>`);
  function renderServers() {
    document.getElementById('preview-server-rows').innerHTML = bookmarks.size ? Array.from(bookmarks, ([name, server]) => `<div class="private-server-row"><span class="private-server-name-block"><span class="private-server-name">${escapeText(name)}</span><small class="preview-note">Place ID: ${escapeText(server.placeId)}</small></span><div class="private-server-actions"><button class="account-button primary" data-server-launch="${escapeText(name)}">${icon('launch')}Launch</button><button class="icon-button" data-server-delete="${escapeText(name)}" aria-label="Remove ${escapeText(name)}">${icon('close')}</button></div></div>`).join('') : '<div class="account-empty-inline"><strong>No saved servers</strong></div>';
  }
  serversView.querySelector('form').addEventListener('submit', event => { event.preventDefault(); const values = new FormData(event.currentTarget); const name = String(values.get('name')).trim(); if (!name) return; bookmarks.set(name, { placeId: '123456789', url: String(values.get('url')) }); event.currentTarget.reset(); renderServers(); });

  const settingsView = createView('Settings', `<main class="settings-layout"><aside class="settings-sidebar"><button class="settings-sidebar-link is-active" data-preview-action="settings-general">General</button><button class="settings-sidebar-link" data-preview-action="settings-storage">Storage</button></aside><div class="settings-content"><div class="settings-content-inner"><section class="settings-section"><h2>General</h2><div class="settings-section-body">${Array.from(settings, ([label, isChecked]) => `<div class="settings-row"><span>${label}</span><label class="settings-toggle"><input type="checkbox" data-preview-setting="${label}" aria-label="${label}" ${isChecked ? 'checked' : ''}></label></div>`).join('')}</div><p class="preview-note">Changes affect this preview only and are cleared on reload.</p></section><section class="settings-section" id="preview-storage"><h2>Storage</h2><p>No account credentials are stored in this preview. The installed app supports device encryption and master passwords.</p></section><p class="preview-note" id="settings-notice" role="status"></p></div></div></main>`);
  settingsView.addEventListener('change', event => { const input = event.target.closest('[data-preview-setting]'); if (!input) return; settings.set(input.dataset.previewSetting, input.checked); document.getElementById('settings-notice').textContent = `${input.dataset.previewSetting}: ${input.checked ? 'enabled' : 'disabled'} in preview`; });

  function navigateWorkspace(name) {
    if (!viewNames.includes(name)) return;
    currentWorkspace = name;
    views.forEach((view, viewName) => { view.hidden = viewName !== name; });
    document.querySelectorAll('.sidebar .rail-button').forEach(button => { const isCurrent = button.getAttribute('aria-label') === name; button.classList.toggle('is-active', isCurrent); if (isCurrent) button.setAttribute('aria-current', 'page'); else button.removeAttribute('aria-current'); });
    if (name === 'Instances') renderInstances();
    if (name === 'Presets') renderPresets();
    if (name === 'Groups') renderMemberships();
    if (name === 'Private Servers') renderServers();
    parent.postMessage({ type: 'workspaceChanged', workspace: name }, '*');
  }

  document.querySelector('.sidebar').addEventListener('click', event => { const button = event.target.closest('.rail-button'); if (!button) return; event.stopImmediatePropagation(); navigateWorkspace(button.getAttribute('aria-label')); }, true);
  layer.addEventListener('click', event => {
    const button = event.target.closest('button');
    if (!button) return;
    if (button.dataset.presetUse) { const preset = previewPresets.get(button.dataset.presetUse); if (!preset) return; document.getElementById('place-id').value = preset.placeId; document.getElementById('job-id').value = preset.jobId; document.getElementById('launch-data').value = preset.launchData; navigateWorkspace('Accounts'); return; }
    if (button.dataset.presetDelete) { const name = button.dataset.presetDelete; confirmPreviewAction('Delete preset', 'Remove this preset from the preview session?', () => { previewPresets.delete(name); renderPresets(); }); return; }
    if (button.dataset.serverLaunch) { nativeDescription('Launch private server'); return; }
    if (button.dataset.serverDelete) { const name = button.dataset.serverDelete; confirmPreviewAction('Remove bookmark', 'Remove this bookmark from the preview session?', () => { bookmarks.delete(name); renderServers(); }); return; }
    const action = button.dataset.previewAction;
    if (!action) return;
    if (action === 'refresh-instances') { renderInstances(); document.getElementById('instance-notice').textContent = 'Preview clients refreshed'; }
    else if (action === 'arrange-windows') document.getElementById('instance-notice').textContent = 'Demo layout: two windows arranged side by side';
    else if (action === 'close-all') confirmPreviewAction('Kill all preview clients', 'Remove every synthetic client from this preview? Real Roblox processes are unaffected.', () => { instances.clear(); renderInstances(); });
    else if (action.startsWith('close-')) { const processId = Number(action.slice(6)); confirmPreviewAction('Kill preview instance', 'Remove this synthetic client from the preview?', () => { instances.delete(processId); renderInstances(); }); }
    else if (action.startsWith('focus-')) document.getElementById('instance-notice').textContent = `${instances.get(Number(action.slice(6))).account}: focused in preview`;
    else if (/^join-\d+$/.test(action)) nativeDescription('Join server');
    else if (action === 'open-folder') nativeDescription('Open presets folder');
    else if (action === 'search-groups') { const query = document.getElementById('preview-group-search').value.trim().toLowerCase(); document.getElementById('group-notice').textContent = !query || 'development studio'.includes(query) || query === '1234567' ? 'Development Studio found in preview' : 'No synthetic groups match this search'; }
    else if (action === 'join-group' || action === 'leave-group') { const key = document.getElementById('preview-group-account').value; if (action === 'join-group') memberships.add(key); else memberships.delete(key); renderMemberships(); document.getElementById('group-notice').textContent = 'Preview membership updated'; }
    else if (action === 'settings-storage') document.getElementById('preview-storage').scrollIntoView({ block: 'start' });
    else if (action === 'settings-general') settingsView.querySelector('.settings-content').scrollTop = 0;
  });
  window.addEventListener('message', event => { if (event.source === parent && event.data?.type === 'navigateWorkspace') navigateWorkspace(event.data.workspace); });
  renderInstances();
  navigateWorkspace(currentWorkspace);
})();
