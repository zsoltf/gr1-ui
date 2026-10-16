import {loadPyodide} from 'https://cdn.jsdelivr.net/pyodide/v0.27.7/full/pyodide.mjs';
self.onmessage = async ({data}) => {
  try {
    self.postMessage({status: 'Loading the browser Python runtime...'});
    const pyodide = await loadPyodide({indexURL: 'https://cdn.jsdelivr.net/pyodide/v0.27.7/full/'});
    self.postMessage({status: 'Loading preparation dependencies...'});
    await pyodide.loadPackage(['micropip', 'pycryptodome']);
    await pyodide.runPythonAsync(`
import micropip
await micropip.install(['xdis==6.3.0', 'pyelftools==0.33', 'click==8.5.0', 'six==1.17.0'])
`);
    pyodide.FS.mkdir('/work');
    const tools = ['ci-bundle.py', 'prepare-core.py', 'inspect_native.py', 'inspect_firmware.py'];
    if (data.usb) tools.push('usb-update.py');
    for (const name of tools) {
      const response = await fetch(new URL('../tools/' + name, self.location.href));
      if (!response.ok) throw new Error('Cannot load preparation tool: ' + name);
      pyodide.FS.writeFile('/work/' + name, await response.text());
    }
    pyodide.FS.writeFile('/work/ui-download.tar.gz', new Uint8Array(data.bundle));
    pyodide.FS.writeFile('/work/firmware.tgz', new Uint8Array(data.firmware));
    self.postMessage({status: 'Verifying files and preparing your private package...'});
    const preparation = data.usb ? `
spec = importlib.util.spec_from_file_location('usb_update', '/work/usb-update.py')
usb = importlib.util.module_from_spec(spec)
spec.loader.exec_module(usb)
json.dumps(usb.prepare(Path('/work/ui-download.tar.gz'), Path('/work/firmware.tgz'), Path('/work/private.tar.gz')))
` : `
spec = importlib.util.spec_from_file_location('prepare_core', '/work/prepare-core.py')
core = importlib.util.module_from_spec(spec)
spec.loader.exec_module(core)
files = core.prepare_files(Path('/work/ui-download.tar.gz'), Path('/work/firmware.tgz'))
core.bundle.write_archive(Path('/work/private.tar.gz'), files, 'gr1-ui')
json.dumps({'engineSha256': hashlib.sha256(files[core.bundle.ENGINE][0]).hexdigest(),
            'packageSha256': hashlib.sha256(Path('/work/private.tar.gz').read_bytes()).hexdigest()})
`;
    const details = JSON.parse(await pyodide.runPythonAsync(`
import sys, json, importlib.util, hashlib
from pathlib import Path
sys.path.insert(0, '/work')
${preparation}
`));
    const bytes = pyodide.FS.readFile('/work/private.tar.gz');
    self.postMessage({package: bytes.buffer, ...details}, [bytes.buffer]);
  } catch (error) {
    const lines = error.message.trim().split('\n');
    self.postMessage({error: lines[lines.length - 1]});
  }
};
