const form = document.querySelector('#prepare');
const status = document.querySelector('#status');
const start = document.querySelector('#start');
const cancel = document.querySelector('#cancel');
const download = document.querySelector('#download');
const verification = document.querySelector('#verification');
let worker = null;
let resultUrl = null;
let generation = 0;
function preparationError(message = '') {
  if (/official firmware hash|unmodified official firmware|Unexpected official archive/i.test(message)) {
    return 'Choose the official 3.4.4 file linked in step 1, then try again.';
  }
  if (message === 'interface-download-unavailable') {
    return 'The interface download is unavailable. Reload this page and try again.';
  }
  return 'Could not prepare your update. Reload this page and try again.';
}
function clearResult() {
  if (resultUrl) URL.revokeObjectURL(resultUrl);
  resultUrl = null;
  download.hidden = true;
  download.removeAttribute('href');
  verification.hidden = true;
  document.querySelector('#package-sha').textContent = '';
}
function finish() {
  if (worker) worker.terminate();
  worker = null;
  start.disabled = false;
  cancel.hidden = true;
}
cancel.addEventListener('click', () => {
  generation++;
  finish();
  status.textContent = 'Cancelled.';
});
for (const input of form.querySelectorAll('input')) input.addEventListener('change', () => {
  generation++;
  finish();
  clearResult();
  status.textContent = [...form.querySelectorAll('input')].every(input => input.files.length)
    ? 'Ready.'
    : '';
});
form.addEventListener('submit', async event => {
  event.preventDefault();
  const current = ++generation;
  clearResult();
  start.disabled = true;
  cancel.hidden = false;
  status.textContent = 'Preparing your update...';
  try {
    const [bundle, firmware] = await Promise.all([
      fetch('gr1-ui-download.tar.gz').then(async response => {
        if (!response.ok) throw Error('interface-download-unavailable');
        return response.arrayBuffer();
      }),
      document.querySelector('#firmware').files[0].arrayBuffer(),
    ]);
    if (current !== generation) return;
    worker = new Worker('worker.js', {type: 'module'});
    worker.onmessage = ({data}) => {
      if (current !== generation) return;
      if (data.status) status.textContent = 'Preparing your update...';
      if (data.error) {
        clearResult();
        status.textContent = preparationError(data.error);
        finish();
      }
      if (data.package) {
        resultUrl = URL.createObjectURL(new Blob([data.package], {type: 'application/gzip'}));
        download.href = resultUrl;
        download.download = data.filename;
        download.hidden = false;
        status.textContent = 'Your USB update is ready. Download it and follow step 3.';
        document.querySelector('#package-sha').textContent = data.packageSha256;
        verification.hidden = false;
        finish();
      }
    };
    worker.onerror = event => {
      if (current !== generation) return;
      clearResult();
      status.textContent = preparationError(event.message);
      finish();
    };
    worker.postMessage({bundle, firmware, usb: true}, [bundle, firmware]);
  } catch (error) {
    if (current !== generation) return;
    clearResult();
    status.textContent = preparationError(error.message);
    finish();
  }
});
