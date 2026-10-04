(() => {
  const $ = id => document.getElementById(id);
  const themes = [...document.querySelectorAll('[data-theme]')];
  function setTheme(theme) {
    if (!['auto', 'day', 'night'].includes(theme)) theme = 'auto';
    document.documentElement.dataset.theme = theme;
    themes.forEach(button => { button.classList.toggle('active', button.dataset.theme === theme); button.setAttribute('aria-pressed', String(button.dataset.theme === theme)); });
    try { localStorage.setItem('rustbin-theme', theme); } catch (_) { /* storage may be disabled */ }
  }
  let saved = 'auto'; try { saved = localStorage.getItem('rustbin-theme') || 'auto'; } catch (_) { /* storage may be disabled */ }
  setTheme(saved);
  themes.forEach(button => button.addEventListener('click', () => setTheme(button.dataset.theme)));
  async function copy(value, button) {
    try {
      if (navigator.clipboard && window.isSecureContext) await navigator.clipboard.writeText(value);
      else { const area = document.createElement('textarea'); area.value = value; area.style.position = 'fixed'; area.style.opacity = '0'; document.body.append(area); area.select(); if (!document.execCommand('copy')) throw Error('Copy failed'); area.remove(); }
      const previous = button.textContent; button.textContent = '[ COPIED ]'; setTimeout(() => button.textContent = previous, 1800);
    } catch (_) { button.textContent = '[ COPY FAILED ]'; }
  }
  const id = decodeURIComponent(location.pathname.slice(1));
  if (id && !id.includes('/')) {
    $('new').classList.add('hidden'); $('paste').classList.remove('hidden');
    $('paste-id').textContent = '/ ' + id;
    fetch('/api/v1/pastes/' + encodeURIComponent(id), {cache:'no-store'}).then(async response => {
      if (!response.ok) throw Error(response.status === 404 ? 'Paste not found or expired.' : 'Could not load paste.');
      return response.json();
    }).then(paste => {
      document.title = (paste.title || paste.id) + ' · RustBin';
      $('paste-title').textContent = paste.title || 'Untitled paste';
      $('paste-meta').textContent = [paste.language.toUpperCase(), paste.content.split('\n').length + ' lines', 'created ' + new Date(paste.created_at).toLocaleString(), paste.expires_at ? 'expires ' + new Date(paste.expires_at).toLocaleString() : 'never expires'].join(' · ');
      $('paste-content').textContent = paste.content;
      $('raw-link').href = '/raw/' + encodeURIComponent(id);
      $('paste-url').textContent = location.href;
      $('copy-paste').onclick = () => copy(location.href, $('copy-paste'));
      $('delete-paste').onclick = async () => {
        const token = prompt('Enter the delete token saved when this paste was created:');
        if (!token) return;
        const response = await fetch('/api/v1/pastes/' + encodeURIComponent(id), {method:'DELETE', headers:{Authorization:'Bearer ' + token}});
        if (response.status === 204) { location.href = '/'; return; }
        $('paste-status').textContent = response.status === 403 ? 'Invalid delete token.' : 'Could not delete paste.';
      };
    }).catch(error => { $('paste-status').textContent = error.message; });
  }
  $('paste-form').addEventListener('submit', async event => {
    event.preventDefault();
    const button = $('paste-form').querySelector('[type=submit]');
    button.disabled = true; $('form-status').textContent = 'Creating…';
    const data = {title:$('title').value, content:$('content').value, language:$('language').value, expiration:$('expiration').value};
    try {
      const response = await fetch('/api/v1/pastes', {method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify(data)});
      const result = await response.json();
      if (!response.ok) throw Error(result.error?.message || 'Could not create paste.');
      const url = new URL(result.url, location.origin).href;
      $('created-url').href = url; $('created-url').textContent = url;
      $('open-created').href = url; $('raw-created').href = result.raw_url;
      $('created-token').textContent = result.delete_token;
      $('copy-created').onclick = () => copy(url, $('copy-created'));
      $('copy-token').onclick = () => copy(result.delete_token, $('copy-token'));
      $('created').classList.remove('hidden'); $('form-status').textContent = 'Paste created.';
      $('created').scrollIntoView({behavior:'smooth'});
    } catch (error) { $('form-status').textContent = error.message; }
    finally { button.disabled = false; }
  });
})();
