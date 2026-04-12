<template>
  <div v-if="show" class="modal modal-open">
    <div class="modal-box" :style="{ width: width }">
      <h3 class="font-bold text-lg">{{ modalTitle }}</h3>
      <div class="py-4">
        <label>Type</label>
        <input v-model="localPattern.type" placeholder="Type" class="input input-bordered w-full mb-2">

        <label>Class</label>
        <select v-model="localPattern.class" class="input input-bordered w-full mb-2">
          <option v-for="classValue in uniqueClassValues" :key="classValue" :value="classValue">{{ classValue }}</option>
        </select>

        <div class="mb-2 w-full flex items-center">
          <label>Description</label>
          <button v-if="showAIAssistant" @click="generateDescription" class="btn btn-primary ml-2">
            <SparklesIcon class="w-5 h-5" />
          </button>
        </div>
        <CodeMirrorEditor v-model="localPattern.description" class="mb-2 w-full"></CodeMirrorEditor>

        <div class="mb-2 w-full flex items-center">
          <label>Regex</label>
          <button v-if="showAIAssistant" @click="generateRegex" class="btn btn-primary ml-2">
            <SparklesIcon class="w-5 h-5" />
          </button>
        </div>
        <CodeMirrorEditor v-model="localPattern.regex" class="mb-2 w-full"></CodeMirrorEditor>

        <label>Callback</label>
        <select v-model="localPattern.callback" class="input input-bordered w-full mb-2">
          <option v-for="callbackValue in uniqueCallbackValues" :key="callbackValue" :value="callbackValue">{{ callbackValue }}</option>
        </select>

        <label>Sample</label>
        <textarea v-model="localPattern.sample" placeholder="Sample" class="textarea textarea-bordered w-full mb-2"></textarea>

        <div class="flex space-x-2">
          <button @click="testPattern" class="btn btn-secondary mb-2">
            <BeakerIcon class="w-5 h-5" />
          </button>
        </div>
      </div>
      <div class="modal-action">
        <button @click="handleModalOk" class="btn btn-primary">{{ okText }}</button>
        <button @click="handleModalCancel" class="btn">{{ cancelText }}</button>
      </div>
    </div>
  </div>
</template>

<script>
import CodeMirrorEditor from './CodeMirrorEditor.vue'
import { SparklesIcon, BeakerIcon } from '@heroicons/vue/24/outline'
import { generate_description, generate_regex } from '../services/api' // Ensure these functions are imported correctly

export default {
  props: ['show', 'modalTitle', 'okText', 'cancelText', 'width', 'currentPattern', 'uniqueClassValues', 'uniqueCallbackValues', 'showAIAssistant'],
  components: { CodeMirrorEditor, SparklesIcon, BeakerIcon },
  data() {
    return {
      localPattern: { ...this.currentPattern }
    }
  },
  methods: {
    handleModalOk() {
      this.$emit('handleModalOk', this.localPattern)
    },
    handleModalCancel() {
      this.$emit('handleModalCancel')
    },
    async generateDescription() {
      try {
        const response = await generate_description(this.localPattern.regex)
        this.localPattern.description = response.description
      } catch (error) {
        console.error('Error generating description:', error)
      }
    },
    async generateRegex() {
      try {
        const response = await generate_regex(this.localPattern.description)
        this.localPattern.regex = response.regex
      } catch (error) {
        console.error('Error generating regex:', error)
      }
    },
    async testPattern() {
      try {
        const response = await fetch('/api/test-pattern', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json'
          },
          body: JSON.stringify({
            regex: this.localPattern.regex,
            sample: this.localPattern.sample
          })
        })
        const result = await response.json()
        alert(result.testResult)
      } catch (error) {
        console.error('Error testing pattern:', error)
      }
    }
  },
  watch: {
    currentPattern(newVal) {
      this.localPattern = { ...newVal }
    }
  }
}
</script>

<style scoped>
.modal-box {
  max-width: 100%; /* Ensure it doesn't exceed the viewport width */
}
</style>
