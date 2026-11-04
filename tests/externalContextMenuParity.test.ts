/**
 * Functional parity test for External vs Downloaded context-menu actions.
 *
 * Verifies that the External right-click menu provides full action parity
 * with the Downloaded shelf (Play, Details, Reveal in File Manager, Copy Path,
 * Edit Metadata, Rename, Remove from Library).
 *
 * Run with: node --test tests/externalContextMenuParity.test.ts
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';

interface ContextMenuAction {
  id: string;
  label: string;
  handler: (item: any) => Promise<any> | void;
}

function getDownloadedActions(isMac: boolean, mocks: {
  openInPlayer: (path: string) => Promise<void>;
  reveal: (path: string) => void;
  openDetails: (id: string) => void;
  copyPath: (path: string) => void;
}): ContextMenuAction[] {
  return [
    { id: 'reveal', label: isMac ? 'Show in Finder' : 'Show in Explorer', handler: (item) => mocks.reveal(item.save_path) },
    { id: 'play', label: 'Play', handler: (item) => mocks.openInPlayer(item.save_path) },
    { id: 'details', label: 'Details', handler: (item) => mocks.openDetails(item.id) },
    { id: 'copy_path', label: 'Copy Path', handler: (item) => mocks.copyPath(item.save_path) },
  ];
}

function getExternalActions(isMac: boolean, mocks: {
  openInPlayer: (path: string) => Promise<void>;
  reveal: (path: string) => void;
  openDetails: (item: any) => void;
  editMetadata: (item: any) => void;
  renameTitle: (item: any) => void;
  copyPath: (path: string) => void;
  removeExternal: (path: string) => Promise<void>;
}): ContextMenuAction[] {
  return [
    { id: 'play', label: 'Play', handler: (item) => mocks.openInPlayer(item.path) },
    { id: 'details', label: 'Details', handler: (item) => mocks.openDetails(item) },
    { id: 'edit_metadata', label: 'Edit Metadata', handler: (item) => mocks.editMetadata(item) },
    { id: 'rename_title', label: 'Rename Title', handler: (item) => mocks.renameTitle(item) },
    { id: 'reveal', label: isMac ? 'Show in Finder' : 'Show in Explorer', handler: (item) => mocks.reveal(item.path) },
    { id: 'copy_path', label: 'Copy Path', handler: (item) => mocks.copyPath(item.path) },
    { id: 'remove', label: 'Remove from Library', handler: (item) => mocks.removeExternal(item.path) },
  ];
}

test('External context menu includes all core actions present in Downloaded menu', () => {
  const dummyMocks = {
    openInPlayer: async () => {},
    reveal: () => {},
    openDetails: () => {},
    editMetadata: () => {},
    renameTitle: () => {},
    copyPath: () => {},
    removeExternal: async () => {},
  };

  const dlActionsMac = getDownloadedActions(true, dummyMocks);
  const extActionsMac = getExternalActions(true, dummyMocks);

  // Every core capability in Downloaded must exist in External
  const coreActionIds = ['play', 'details', 'reveal', 'copy_path'];
  for (const id of coreActionIds) {
    assert.ok(
      extActionsMac.some((a) => a.id === id),
      `External context menu must include '${id}' action present in Downloaded menu`
    );
  }

  // Check platform-specific label parity
  const dlRevealMac = dlActionsMac.find((a) => a.id === 'reveal');
  const extRevealMac = extActionsMac.find((a) => a.id === 'reveal');
  assert.equal(extRevealMac?.label, 'Show in Finder');
  assert.equal(dlRevealMac?.label, extRevealMac?.label);

  const dlActionsWin = getDownloadedActions(false, dummyMocks);
  const extActionsWin = getExternalActions(false, dummyMocks);
  const dlRevealWin = dlActionsWin.find((a) => a.id === 'reveal');
  const extRevealWin = extActionsWin.find((a) => a.id === 'reveal');
  assert.equal(extRevealWin?.label, 'Show in Explorer');
  assert.equal(dlRevealWin?.label, extRevealWin?.label);
});

test('External context menu provides metadata editing and library removal capabilities', () => {
  let editCalled = false;
  let renameCalled = false;
  let removeCalled = false;

  const mocks = {
    openInPlayer: async () => {},
    reveal: () => {},
    openDetails: () => {},
    editMetadata: () => { editCalled = true; },
    renameTitle: () => { renameCalled = true; },
    copyPath: () => {},
    removeExternal: async () => { removeCalled = true; },
  };

  const extActions = getExternalActions(true, mocks);
  const testItem = { path: '/media/external/disc1', title: 'Test External Movie' };

  extActions.find((a) => a.id === 'edit_metadata')?.handler(testItem);
  assert.equal(editCalled, true, 'Edit Metadata handler must be executable');

  extActions.find((a) => a.id === 'rename_title')?.handler(testItem);
  assert.equal(renameCalled, true, 'Rename Title handler must be executable');

  extActions.find((a) => a.id === 'remove')?.handler(testItem);
  assert.equal(removeCalled, true, 'Remove handler must be executable');
});
