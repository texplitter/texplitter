<template>
    <div ref="editor" class="editor w-full h-auto"></div>
  </template>
  
  <script>
  import { onMounted, ref, watch } from 'vue'
  import { EditorState } from '@codemirror/state'
  import { EditorView, keymap } from '@codemirror/view'
  import { defaultKeymap } from '@codemirror/commands'
  import { javascript } from '@codemirror/lang-javascript'
  
  export default {
    name: 'CodeMirrorEditor',
    props: {
      modelValue: {
        type: String,
        required: true,
      },
    },
    emits: ['update:modelValue'],
    setup(props, { emit }) {
      const editor = ref(null)
      let view = null
  
      onMounted(() => {
        view = new EditorView({
          state: EditorState.create({
            doc: props.modelValue,
            extensions: [
              keymap.of(defaultKeymap),
              javascript(),
              EditorView.updateListener.of((update) => {
                if (update.docChanged) {
                  emit('update:modelValue', update.state.doc.toString())
                }
              }),
            ],
          }),
          parent: editor.value,
        })
      })
  
      watch(
        () => props.modelValue,
        (newValue) => {
          if (view) {
            const currentValue = view.state.doc.toString()
            if (newValue !== currentValue) {
              view.dispatch({
                changes: { from: 0, to: currentValue.length, insert: newValue },
              })
            }
          }
        }
      )
  
      return { editor }
    },
  }
  </script>
  
  <style>
  .editor {
    border: 1px solid #ddd;
    border-radius: 4px;
    padding: 8px;
    min-height: 200px;
    font-family: 'Courier New', Courier, monospace;
    height: auto; /* Ensure the height adjusts based on content */
    overflow-y: auto; /* Add overflow-y to manage content overflow */
    box-sizing: border-box; /* Ensure padding and border are included in the element's total width and height */
  }
  .CodeMirror {
    height: auto; /* Ensure the CodeMirror instance adjusts its height based on content */
    min-height: 150px; /* Adjust the minimum height as needed */
  }
  </style>
  