<script lang="ts">
  import { markdown } from '@codemirror/lang-markdown';
  import { EditorState } from '@codemirror/state';
  import {
    EditorView,
    keymap,
    lineNumbers,
    highlightActiveLineGutter,
    highlightSpecialChars,
    drawSelection,
    dropCursor,
    rectangularSelection,
    highlightActiveLine,
  } from '@codemirror/view';
  import {
    foldGutter,
    indentOnInput,
    syntaxHighlighting,
    defaultHighlightStyle,
    bracketMatching,
    foldKeymap,
    indentUnit,
  } from '@codemirror/language';
  import { history, defaultKeymap, historyKeymap, indentWithTab } from '@codemirror/commands';
  import { searchKeymap, highlightSelectionMatches } from '@codemirror/search';
  import {
    autocompletion,
    completionKeymap,
    closeBrackets,
    closeBracketsKeymap,
  } from '@codemirror/autocomplete';
  import { onMount } from 'svelte';
  import { SvelteMap } from 'svelte/reactivity';
  import {
    Bold,
    Italic,
    Heading,
    Quote,
    Code,
    Link,
    List,
    ListOrdered,
    ListTodo,
  } from '@lucide/svelte';

  const states = new SvelteMap<string, EditorState>();
  let {
    documentId,
    value = '',
    showLineNumbers = true,
    tabSize = 4,
    spellcheck = false,
    onchange,
    onselectionchange,
  }: {
    documentId: string;
    value?: string;
    showLineNumbers?: boolean;
    tabSize?: number;
    spellcheck?: boolean;
    onchange: (value: string) => void;
    onselectionchange: (selection: { anchor: number; head: number }) => void;
  } = $props();
  let host: HTMLDivElement;
  let view: EditorView | undefined;
  let localValue = $state('');
  let currentId = '';

  let lastSln: boolean | undefined = undefined;
  let lastTs: number | undefined = undefined;
  let lastSc: boolean | undefined = undefined;

  function createState(content: string, selection?: any) {
    const ext = [
      highlightSpecialChars(),
      history(),
      drawSelection(),
      dropCursor(),
      EditorState.allowMultipleSelections.of(true),
      indentOnInput(),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      bracketMatching(),
      closeBrackets(),
      autocompletion(),
      rectangularSelection(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      markdown(),
      keymap.of([
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        ...foldKeymap,
        ...completionKeymap,
        indentWithTab,
      ]),
      EditorView.lineWrapping,
      indentUnit.of(' '.repeat(tabSize)),
      EditorView.contentAttributes.of({ spellcheck: spellcheck ? 'true' : 'false' }),
      EditorView.updateListener.of((update) => {
        if (update.selectionSet) {
          onselectionchange({
            anchor: update.state.selection.main.anchor,
            head: update.state.selection.main.head,
          });
        }
        if (!update.docChanged) return;
        localValue = update.state.doc.toString();
        onchange(localValue);
      }),
    ];

    if (showLineNumbers) {
      ext.push(lineNumbers());
      ext.push(highlightActiveLineGutter());
      ext.push(foldGutter());
    }

    return EditorState.create({
      doc: content,
      selection,
      extensions: ext,
    });
  }

  function insertFormat(
    type:
      | 'bold'
      | 'italic'
      | 'heading'
      | 'quote'
      | 'code'
      | 'link'
      | 'list'
      | 'ordered-list'
      | 'task-list'
  ) {
    if (!view) return;
    const { state, dispatch } = view;
    const { anchor, head } = state.selection.main;
    const from = Math.min(anchor, head);
    const to = Math.max(anchor, head);
    const selectedText = state.doc.sliceString(from, to);

    let replacement = '';
    let cursorOffset = 0;

    switch (type) {
      case 'bold':
        replacement = `**${selectedText || 'bold text'}**`;
        cursorOffset = selectedText ? 0 : -2;
        break;
      case 'italic':
        replacement = `*${selectedText || 'italic text'}*`;
        cursorOffset = selectedText ? 0 : -1;
        break;
      case 'heading':
        replacement = `### ${selectedText || 'Heading'}`;
        cursorOffset = selectedText ? 0 : 0;
        break;
      case 'quote':
        replacement = `> ${selectedText || 'quote'}`;
        cursorOffset = selectedText ? 0 : 0;
        break;
      case 'code':
        replacement = `\`\`\`\n${selectedText || 'code'}\n\`\`\``;
        cursorOffset = selectedText ? 0 : 0;
        break;
      case 'link':
        replacement = `[${selectedText || 'link text'}](https://)`;
        cursorOffset = selectedText ? -1 : -11;
        break;
      case 'list':
        replacement = `- ${selectedText || 'list item'}`;
        cursorOffset = selectedText ? 0 : 0;
        break;
      case 'ordered-list':
        replacement = `1. ${selectedText || 'list item'}`;
        cursorOffset = selectedText ? 0 : 0;
        break;
      case 'task-list':
        replacement = `- [ ] ${selectedText || 'task item'}`;
        cursorOffset = selectedText ? 0 : 0;
        break;
    }

    dispatch({
      changes: { from, to, insert: replacement },
      selection: { anchor: from + replacement.length + cursorOffset },
    });
    view.focus();
  }

  onMount(() => {
    localValue = value;
    currentId = documentId;
    lastSln = showLineNumbers;
    lastTs = tabSize;
    lastSc = spellcheck;
    view = new EditorView({
      parent: host,
      state: states.get(documentId) ?? createState(value),
    });

    return () => view?.destroy();
  });

  $effect(() => {
    if (!view) return;

    if (documentId !== currentId) {
      if (currentId) {
        states.set(currentId, view.state);
      }
      currentId = documentId;
      const next = states.get(documentId) ?? createState(value);
      localValue = next.doc.toString();
      view.setState(next);
      lastSln = showLineNumbers;
      lastTs = tabSize;
      lastSc = spellcheck;
      return;
    }

    if (showLineNumbers !== lastSln || tabSize !== lastTs || spellcheck !== lastSc) {
      lastSln = showLineNumbers;
      lastTs = tabSize;
      lastSc = spellcheck;

      const currentDoc = view.state.doc.toString();
      const currentSelection = view.state.selection;
      const nextState = createState(currentDoc, currentSelection);
      view.setState(nextState);
      return;
    }

    if (value === localValue) return;
    localValue = value;
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } });
  });
</script>

<div class="editor-container">
  <div class="formatting-toolbar">
    <button class="toolbar-btn" title="Bold" onclick={() => insertFormat('bold')}
      ><Bold size={14} /></button
    >
    <button class="toolbar-btn" title="Italic" onclick={() => insertFormat('italic')}
      ><Italic size={14} /></button
    >
    <button class="toolbar-btn" title="Heading" onclick={() => insertFormat('heading')}
      ><Heading size={14} /></button
    >
    <span class="toolbar-separator"></span>
    <button class="toolbar-btn" title="Quote" onclick={() => insertFormat('quote')}
      ><Quote size={14} /></button
    >
    <button class="toolbar-btn" title="Code block" onclick={() => insertFormat('code')}
      ><Code size={14} /></button
    >
    <button class="toolbar-btn" title="Link" onclick={() => insertFormat('link')}
      ><Link size={14} /></button
    >
    <span class="toolbar-separator"></span>
    <button class="toolbar-btn" title="Unordered list" onclick={() => insertFormat('list')}
      ><List size={14} /></button
    >
    <button class="toolbar-btn" title="Ordered list" onclick={() => insertFormat('ordered-list')}
      ><ListOrdered size={14} /></button
    >
    <button class="toolbar-btn" title="Task list" onclick={() => insertFormat('task-list')}
      ><ListTodo size={14} /></button
    >
  </div>
  <div class="editor-host" bind:this={host}></div>
</div>
