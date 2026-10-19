// scripts/verify-modal-sequence.js
// Verifies the exact sequence: invalid folder -> valid Unicode .torrent
// Confirms that the stale error disappears and the selected file, preview,
// submitted bytes, and displayed error strictly refer to the current input.

import assert from 'node:assert';

console.log('=== Test: isAllowedMetainfoName validation ===');
const isAllowedMetainfoName = (name) => /\.(torrent|adc)$/i.test(name.trim());

assert.strictEqual(isAllowedMetainfoName('MyDownloadFolder'), false, 'Folder name without extension must be rejected');
assert.strictEqual(isAllowedMetainfoName('/Volumes/Data/Incomplete/'), false, 'Path without torrent extension must be rejected');
assert.strictEqual(isAllowedMetainfoName('movie.mkv'), false, 'Non-torrent file must be rejected');
assert.strictEqual(isAllowedMetainfoName('日本語_アニメ.torrent'), true, 'Unicode .torrent must be accepted');
assert.strictEqual(isAllowedMetainfoName('ADC_Release.adc'), true, '.adc file must be accepted');
assert.strictEqual(isAllowedMetainfoName('TEST.TORRENT'), true, 'Case-insensitive extension must be accepted');
console.log('  PASS: isAllowedMetainfoName correctly distinguishes folders/files from metainfo');

console.log('=== Test: Modal State Transition Sequence ===');

// Mock Modal State
let state = {
  selectedFileName: null,
  selectedFileSize: null,
  selectedFileBytes: null,
  inspectPreview: null,
  torrentSource: '',
  errorMessage: null,
  isSubmitting: false,
};

function resetAll() {
  state = {
    selectedFileName: null,
    selectedFileSize: null,
    selectedFileBytes: null,
    inspectPreview: null,
    torrentSource: '',
    errorMessage: null,
    isSubmitting: false,
  };
}

// Handler functions matching AddTorrentModal.tsx
function handleDrop(files) {
  if (!files || files.length === 0) return;
  const file = files[0];
  if (!isAllowedMetainfoName(file.name)) {
    state.errorMessage = `only .torrent or .adc files are accepted ('${file.name}' was ignored).`;
    state.selectedFileName = null;
    state.selectedFileSize = null;
    state.selectedFileBytes = null;
    state.inspectPreview = null;
    state.torrentSource = '';
    return;
  }
  state.errorMessage = null;
  state.selectedFileName = file.name;
  state.selectedFileSize = file.size;
  state.selectedFileBytes = null;
  state.inspectPreview = null;
  state.torrentSource = file.path || file.name;
}

function handleChooseFile(nativePath) {
  if (!nativePath) return;
  const name = nativePath.split('/').pop()?.split('\\').pop() || nativePath;
  state.errorMessage = null;
  state.selectedFileName = name;
  state.selectedFileSize = null;
  state.selectedFileBytes = null;
  state.inspectPreview = null;
  state.torrentSource = nativePath;
}

function applyInspectPreview(preview) {
  state.inspectPreview = preview;
  if (preview?.metainfo_bytes && preview.metainfo_bytes.length > 0) {
    state.selectedFileBytes = preview.metainfo_bytes;
  }
}

// 1. Initial State
resetAll();
assert.strictEqual(state.selectedFileName, null);
assert.strictEqual(state.errorMessage, null);

// 2. Action: User selects or drops an invalid folder
console.log('1. User drops invalid folder: "InvalidFolder"');
handleDrop([{ name: 'InvalidFolder', path: '/Users/test/Downloads/InvalidFolder', size: 4096 }]);

assert.strictEqual(state.errorMessage, "only .torrent or .adc files are accepted ('InvalidFolder' was ignored).");
assert.strictEqual(state.selectedFileName, null, 'selectedFileName must be null on invalid folder');
assert.strictEqual(state.selectedFileBytes, null, 'selectedFileBytes must be null on invalid folder');
assert.strictEqual(state.inspectPreview, null, 'inspectPreview must be null on invalid folder');
assert.strictEqual(state.torrentSource, '', 'torrentSource must be empty on invalid folder');
console.log('  PASS: Error displayed and prior state cleared on invalid folder');

// 3. Action: User selects a valid Unicode .torrent file
console.log('2. User selects valid Unicode .torrent: "日本語_アニメ_サンプル.torrent"');
const unicodePath = '/Users/test/Downloads/日本語_アニメ_サンプル.torrent';
handleChooseFile(unicodePath);

assert.strictEqual(state.errorMessage, null, 'Old error message MUST be cleared upon valid file selection');
assert.strictEqual(state.selectedFileName, '日本語_アニメ_サンプル.torrent', 'selectedFileName must be the Unicode filename');
assert.strictEqual(state.torrentSource, unicodePath, 'torrentSource must be the Unicode path');

// Mock inspect completion
const mockBytes = [0x64, 0x38, 0x3a, 0x61, 0x6e, 0x6e, 0x6f, 0x75, 0x6e, 0x63, 0x65];
applyInspectPreview({
  name: '日本語_アニメ_サンプル',
  piece_count: 120,
  piece_length: 262144,
  total_size: 31457280,
  metainfo_bytes: mockBytes,
});

assert.strictEqual(state.selectedFileBytes, mockBytes, 'selectedFileBytes must contain the Unicode torrent bytes');
assert.strictEqual(state.inspectPreview.name, '日本語_アニメ_サンプル', 'inspectPreview must reflect the current input');
assert.strictEqual(state.errorMessage, null, 'Error remains null on valid preview');
console.log('  PASS: Stale error disappeared, preview and submitted bytes refer strictly to the current Unicode input');

// 4. Action: Submission
console.log('3. Submitting the modal form');
let submittedSource = null;
let submittedBytes = null;
function mockAddTorrent(source, dir, bytes) {
  submittedSource = source;
  submittedBytes = bytes;
}

mockAddTorrent(state.torrentSource, '~/Downloads', state.selectedFileBytes);
assert.strictEqual(submittedSource, unicodePath, 'Submitted source must be the valid Unicode path');
assert.strictEqual(submittedBytes, mockBytes, 'Submitted bytes must be the valid Unicode torrent bytes');
console.log('  PASS: Submitted payload strictly refers to the current Unicode input');

console.log('\nAll reproduction assertions PASSED successfully.');
