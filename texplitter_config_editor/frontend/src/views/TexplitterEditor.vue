<template>
  <div class="flex flex-col h-screen overflow-hidden">
    <Navbar
      @createPattern="handleCreatePattern"
      @editPattern="openEditModal"
      @deletePattern="deletePattern"
      @moveUp="moveUp"
      @moveDown="moveDown"
      @selectRuleset="handleSelectRuleset"
      @createRuleset="handleCreateRuleset"
      @duplicateRuleset="handleDuplicateRuleset"
      @renameRuleset="handleRenameRuleset"
      @deleteRuleset="handleDeleteRuleset"
      :selectedIndex="selectedIndex"
      :patterns="patterns"
      :rulesets="rulesets"
      :currentRuleset="currentRuleset"
    />
    <div class="flex-grow overflow-hidden p-4 w-full">
      <PatternTable
        :patterns="patterns"
        @selectRow="selectRow"
        :selectedIndex="selectedIndex"
      />
      <PatternModal
        :show="showModal"
        :modalTitle="modalTitle"
        :okText="okText"
        :cancelText="cancelText"
        :width="modalWidth"
        :currentPattern="currentPattern"
        :uniqueClassValues="uniqueClassValues"
        :uniqueCallbackValues="uniqueCallbackValues"
        :showAIAssistant="showAIAssistant"
        @handleModalOk="handleModalOk"
        @handleModalCancel="handleModalCancel"
        @callAIAssistant="callAIAssistant"
        @testPattern="testPattern"
      />
    </div>
  </div>
</template>

<script>
import Navbar from '../components/Navbar.vue'
import PatternTable from '../components/PatternTable.vue'
import PatternModal from '../components/PatternModal.vue'
import {
  fetchRulesets,
  fetchPatterns,
  fetchUniqueClassValues,
  fetchUniqueCallbackValues,
  checkAIAssistant,
  createRuleset,
  deleteRuleset,
  renameRuleset,
  rulesetPatternsUrl,
  rulesetPatternsOrderUrl
} from '../services/api'
import axios from 'axios'

// Rule for a valid ruleset stem. Mirrors the backend allow-list —
// enforcing it in the client too means the user sees the error before
// a round-trip. The backend still validates, so this is belt-and-braces.
const RULESET_NAME_RE = /^[a-zA-Z0-9_-]+$/
const PROTECTED_RULESETS = new Set(['default'])

// localStorage key for the last-used ruleset. Keeping the selection
// sticky across reloads is worth the ~10 lines of persistence code —
// it's the number-one reason someone hits refresh in the middle of
// editing a non-default ruleset and ends up back on the default.
const RULESET_STORAGE_KEY = 'texplitter.currentRuleset'

export default {
  components: { Navbar, PatternTable, PatternModal },
  data() {
    return {
      rulesets: [],
      currentRuleset: null,
      patterns: [],
      currentPattern: {
        type: '',
        class: 'TRANSLATABLE',
        description: '',
        regex: '',
        callback: 'default_callback',
        sample: ''
      },
      selectedIndex: null,
      showModal: false,
      modalTitle: 'Add New Pattern',
      okText: 'Save',
      cancelText: 'Cancel',
      modalWidth: 'auto',
      uniqueClassValues: [],
      uniqueCallbackValues: [],
      showAIAssistant: false,
      insertPosition: 'end' // Default insert position
    }
  },
  methods: {
    async loadRulesets() {
      console.log('Fetching rulesets...')
      this.rulesets = await fetchRulesets()
      console.log('Rulesets fetched:', this.rulesets)

      // Resolve the current ruleset in three tiers, in priority order:
      //   1. localStorage, if the remembered name still exists on the
      //      server (a user may have deleted the file from disk since
      //      their last session).
      //   2. 'default', if present — it's the conventional baseline.
      //   3. the first entry in the sorted list, if the server has
      //      any rulesets at all.
      // If all three fail the app simply shows an empty state; the
      // navbar dropdown will still render an empty list and every api
      // call bails early on a null selection.
      const stored = localStorage.getItem(RULESET_STORAGE_KEY)
      if (stored && this.rulesets.includes(stored)) {
        this.currentRuleset = stored
      } else if (this.rulesets.includes('default')) {
        this.currentRuleset = 'default'
      } else if (this.rulesets.length > 0) {
        this.currentRuleset = this.rulesets[0]
      } else {
        this.currentRuleset = null
      }
      console.log('Active ruleset:', this.currentRuleset)
    },
    async reloadAll() {
      // Called on initial mount and on every ruleset switch. Each
      // ruleset has its own patterns / unique classes / unique
      // callbacks — none of these survive a selection change, so we
      // refetch the whole set rather than trying to be clever about
      // partial invalidation.
      if (!this.currentRuleset) {
        this.patterns = []
        this.uniqueClassValues = []
        this.uniqueCallbackValues = []
        return
      }
      this.selectedIndex = null
      await Promise.all([
        this.fetchPatterns(),
        this.fetchUniqueClassValues(),
        this.fetchUniqueCallbackValues()
      ])
    },
    async fetchPatterns() {
      console.log(`Fetching patterns for ${this.currentRuleset}...`)
      this.patterns = await fetchPatterns(this.currentRuleset)
      console.log('Patterns fetched:', this.patterns)
    },
    async fetchUniqueClassValues() {
      console.log(`Fetching unique class values for ${this.currentRuleset}...`)
      this.uniqueClassValues = await fetchUniqueClassValues(this.currentRuleset)
      console.log('Unique class values fetched:', this.uniqueClassValues)
    },
    async fetchUniqueCallbackValues() {
      console.log(`Fetching unique callback values for ${this.currentRuleset}...`)
      this.uniqueCallbackValues = await fetchUniqueCallbackValues(this.currentRuleset)
      console.log('Unique callback values fetched:', this.uniqueCallbackValues)
    },
    async checkAIAssistant() {
      console.log('Checking AI assistant availability...')
      this.showAIAssistant = await checkAIAssistant()
      console.log('AI assistant availability:', this.showAIAssistant)
    },
    async handleSelectRuleset(name) {
      if (name === this.currentRuleset) return
      console.log(`Switching to ruleset ${name}`)
      this.currentRuleset = name
      localStorage.setItem(RULESET_STORAGE_KEY, name)
      await this.reloadAll()
    },
    // Ruleset lifecycle handlers. These use `window.prompt` for name
    // capture — deliberately minimal. A proper modal would be nicer but
    // the flow is short enough that a prompt doesn't hurt, and keeps
    // this change scoped to multi-ruleset rather than sprawling into a
    // modal redesign.
    promptRulesetName(label, defaultValue = '') {
      // eslint-disable-next-line no-alert
      const raw = window.prompt(label, defaultValue)
      if (raw === null) return null // user cancelled
      const name = raw.trim()
      if (!name) {
        // eslint-disable-next-line no-alert
        window.alert('Ruleset name cannot be empty.')
        return null
      }
      if (!RULESET_NAME_RE.test(name)) {
        // eslint-disable-next-line no-alert
        window.alert(
          `Invalid ruleset name "${name}". Use only letters, digits, underscores, and hyphens.`
        )
        return null
      }
      return name
    },
    async handleCreateRuleset() {
      const name = this.promptRulesetName('Name for the new (empty) ruleset:')
      if (!name) return
      try {
        await createRuleset(name)
      } catch (error) {
        // eslint-disable-next-line no-alert
        window.alert(`Could not create ruleset: ${error.message}`)
        return
      }
      await this.loadRulesets()
      await this.handleSelectRuleset(name)
    },
    async handleDuplicateRuleset() {
      if (!this.currentRuleset) return
      const name = this.promptRulesetName(
        `Duplicate "${this.currentRuleset}" as:`,
        `${this.currentRuleset}-copy`
      )
      if (!name) return
      try {
        await createRuleset(name, this.currentRuleset)
      } catch (error) {
        // eslint-disable-next-line no-alert
        window.alert(`Could not duplicate ruleset: ${error.message}`)
        return
      }
      await this.loadRulesets()
      await this.handleSelectRuleset(name)
    },
    async handleRenameRuleset() {
      if (!this.currentRuleset) return
      if (PROTECTED_RULESETS.has(this.currentRuleset)) {
        // eslint-disable-next-line no-alert
        window.alert(`"${this.currentRuleset}" is protected and cannot be renamed.`)
        return
      }
      const newName = this.promptRulesetName(
        `Rename "${this.currentRuleset}" to:`,
        this.currentRuleset
      )
      if (!newName || newName === this.currentRuleset) return
      const oldName = this.currentRuleset
      try {
        await renameRuleset(oldName, newName)
      } catch (error) {
        // eslint-disable-next-line no-alert
        window.alert(`Could not rename ruleset: ${error.message}`)
        return
      }
      await this.loadRulesets()
      await this.handleSelectRuleset(newName)
    },
    async handleDeleteRuleset() {
      if (!this.currentRuleset) return
      if (PROTECTED_RULESETS.has(this.currentRuleset)) {
        // eslint-disable-next-line no-alert
        window.alert(`"${this.currentRuleset}" is protected and cannot be deleted.`)
        return
      }
      // eslint-disable-next-line no-alert
      const confirmed = window.confirm(
        `Delete ruleset "${this.currentRuleset}"? This cannot be undone.`
      )
      if (!confirmed) return
      const deleted = this.currentRuleset
      try {
        await deleteRuleset(deleted)
      } catch (error) {
        // eslint-disable-next-line no-alert
        window.alert(`Could not delete ruleset: ${error.message}`)
        return
      }
      // Clear the remembered selection so loadRulesets() falls through
      // to 'default' or first-alphabetical.
      localStorage.removeItem(RULESET_STORAGE_KEY)
      this.currentRuleset = null
      await this.loadRulesets()
      await this.reloadAll()
    },
    handleCreatePattern(position) {
      console.log(`TexplitterEditor received createPattern event with position: ${position}`)
      this.openCreateModal(position)
    },
    openCreateModal(position = 'end') {
      console.log('Opening modal at position:', position)
      this.resetCurrentPattern()
      this.modalTitle = 'Add New Pattern'
      this.showModal = true
      this.insertPosition = position // Set the insert position based on user selection
      console.log('Modal state:', this.showModal)
    },
    openEditModal() {
      if (this.selectedIndex !== null) {
        this.currentPattern = { ...this.patterns[this.selectedIndex] }
        this.modalTitle = 'Edit Pattern'
        this.showModal = true
        console.log('Opening edit modal:', this.currentPattern)
      }
    },
    async deletePattern() {
      if (this.selectedIndex === null || !this.currentRuleset) return
      await axios.delete(
        `${rulesetPatternsUrl(this.currentRuleset)}/${this.selectedIndex}`
      )
      await this.fetchPatterns()
    },
    async moveUp() {
      if (this.selectedIndex !== null && this.selectedIndex > 0) {
        const patterns = [...this.patterns]
        const [removed] = patterns.splice(this.selectedIndex, 1)
        patterns.splice(this.selectedIndex - 1, 0, removed)
        this.patterns = patterns
        this.selectedIndex -= 1
        await this.savePatternsOrder()
      }
    },
    async moveDown() {
      if (this.selectedIndex !== null && this.selectedIndex < this.patterns.length - 1) {
        const patterns = [...this.patterns]
        const [removed] = patterns.splice(this.selectedIndex, 1)
        patterns.splice(this.selectedIndex + 1, 0, removed)
        this.patterns = patterns
        this.selectedIndex += 1
        await this.savePatternsOrder()
      }
    },
    async savePatternsOrder() {
      if (!this.currentRuleset) return
      try {
        console.log(`Saving pattern order for ${this.currentRuleset}:`, this.patterns)
        const formattedPatterns = this.patterns.map(pattern => ({
          type: pattern.type,
          class: pattern.class,
          description: pattern.description,
          regex: pattern.regex,
          callback: pattern.callback,
          sample: pattern.sample
        }))
        await axios.put(
          rulesetPatternsOrderUrl(this.currentRuleset),
          { patterns: formattedPatterns }
        )
        console.log('Patterns order saved')
      } catch (error) {
        console.error('Error saving patterns order:', error.response ? error.response.data : error)
      }
    },
    selectRow(index) {
      if (this.selectedIndex === index) {
        this.selectedIndex = null
      } else {
        this.selectedIndex = index
      }
    },
    resetCurrentPattern() {
      this.currentPattern = {
        type: '',
        class: 'TRANSLATABLE',
        description: '',
        regex: '',
        callback: 'default_callback',
        sample: ''
      }
    },
    async handleModalOk(localPattern) {
      if (!this.currentRuleset) return
      try {
        console.log('Saving pattern:', localPattern)
        let patterns = [...this.patterns]
        const patternsUrl = rulesetPatternsUrl(this.currentRuleset)
        if (this.modalTitle === 'Add New Pattern') {
          switch (this.insertPosition) {
            case 'beginning':
              patterns.unshift(localPattern)
              break
            case 'end':
              patterns.push(localPattern)
              break
            case 'before':
              if (this.selectedIndex !== null) {
                patterns.splice(this.selectedIndex, 0, localPattern)
              } else {
                patterns.push(localPattern)
              }
              break
            case 'after':
              if (this.selectedIndex !== null) {
                patterns.splice(this.selectedIndex + 1, 0, localPattern)
              } else {
                patterns.push(localPattern)
              }
              break
            default:
              patterns.push(localPattern)
          }
          this.patterns = patterns
          await axios.post(patternsUrl, localPattern)
        } else {
          patterns[this.selectedIndex] = localPattern
          this.patterns = patterns
          await axios.put(`${patternsUrl}/${this.selectedIndex}`, localPattern)
        }
        await this.savePatternsOrder()  // Save the order after updating
        this.showModal = false
        console.log('Modal closed after saving pattern')
      } catch (error) {
        console.error('Error saving pattern:', error.response ? error.response.data : error)
      }
    },
    handleModalCancel() {
      this.showModal = false
      console.log('Modal canceled')
    },
    async callAIAssistant(action, data) {
      const response = await axios.post(`/api/ai-assistant/${action}`, data)
      this.currentPattern.description = response.data.description
      this.currentPattern.regex = response.data.regex
    },
    async testPattern(data) {
      const response = await axios.post('/api/test-pattern', data)
      return response.data.testResult
    }
  },
  async mounted() {
    await this.loadRulesets()
    await this.reloadAll()
    await this.checkAIAssistant()
  }
}
</script>
