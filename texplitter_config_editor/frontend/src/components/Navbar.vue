<template>
  <div class="navbar bg-base-400 w-full">
    <div class="flex-1 flex items-center">
      <a class="btn btn-ghost normal-case text-xl" href="#">
        <img :src="logo" alt="Text Cutter" class="h-8 mr-2" />
      </a>
      <!--
        Ruleset picker. Each ruleset is a standalone file under
        `src/texplitter/config/rules/`; switching swaps the entire
        patterns list (no concatenation — first-match-wins ordering
        makes composition semantically broken). The parent view handles
        persistence and reload on selection.
      -->
      <div class="dropdown tooltip tooltip-bottom ml-2" data-tip="Select Ruleset">
        <button
          class="btn btn-ghost dropdown-toggle"
          @click="toggleRulesetDropdown"
        >
          <FolderIcon class="w-5 h-5 mr-1" />
          <span class="normal-case">{{ currentRuleset || 'no ruleset' }}</span>
        </button>
        <ul
          v-show="rulesetDropdownOpen"
          class="dropdown-content menu p-2 shadow bg-base-100 rounded-box w-56 z-50"
        >
          <li v-for="name in rulesets" :key="name">
            <a
              @click="handleRulesetClick(name)"
              :class="{ 'font-bold': name === currentRuleset }"
              tabindex="0"
            >
              {{ name }}
            </a>
          </li>
          <li v-if="rulesets && rulesets.length > 0" class="menu-title mt-2">
            <span>Manage</span>
          </li>
          <li>
            <a @click="handleCreateRulesetClick" tabindex="0">
              <PlusIcon class="w-4 h-4 mr-1" /> New ruleset…
            </a>
          </li>
          <li :class="{ 'disabled': !currentRuleset }">
            <a
              @click="handleDuplicateRulesetClick"
              :class="{ 'pointer-events-none': !currentRuleset }"
              tabindex="0"
            >
              <DocumentDuplicateIcon class="w-4 h-4 mr-1" /> Duplicate current…
            </a>
          </li>
          <li :class="{ 'disabled': !canMutateCurrent }">
            <a
              @click="handleRenameRulesetClick"
              :class="{ 'pointer-events-none': !canMutateCurrent }"
              tabindex="0"
            >
              <PencilIcon class="w-4 h-4 mr-1" /> Rename current…
            </a>
          </li>
          <li :class="{ 'disabled': !canMutateCurrent }">
            <a
              @click="handleDeleteRulesetClick"
              :class="{ 'pointer-events-none': !canMutateCurrent, 'text-error': canMutateCurrent }"
              tabindex="0"
            >
              <TrashIcon class="w-4 h-4 mr-1" /> Delete current…
            </a>
          </li>
        </ul>
      </div>
    </div>
    <div class="flex-none space-x-2">
      <div class="dropdown tooltip tooltip-bottom" data-tip="Add New Pattern">
        <button class="btn btn-primary dropdown-toggle" @click="toggleDropdown">
          <PlusIcon class="w-5 h-5" />
        </button>
        <ul v-show="dropdownOpen" class="dropdown-content menu p-2 shadow bg-base-100 rounded-box w-52 z-50">
          <li><a @click="handleOptionClick('beginning')" tabindex="0">At Beginning</a></li>
          <li><a @click="handleOptionClick('end')" tabindex="0">At End</a></li>
          <li :class="{'disabled': selectedIndex === null}">
            <a @click="handleOptionClick('before')" :class="{'pointer-events-none': selectedIndex === null}" tabindex="0">
              Before Selected
            </a>
          </li>
          <li :class="{'disabled': selectedIndex === null}">
            <a @click="handleOptionClick('after')" :class="{'pointer-events-none': selectedIndex === null}" tabindex="0">
              After Selected
            </a>
          </li>
        </ul>
      </div>
      <div class="tooltip tooltip-bottom" data-tip="Edit Selected Pattern">
        <button @click="$emit('editPattern')" :disabled="selectedIndex === null" class="btn btn-secondary" :class="{'btn-disabled': selectedIndex === null}">
          <PencilIcon class="w-5 h-5" />
        </button>
      </div>
      <div class="tooltip tooltip-bottom" data-tip="Move Up">
        <button @click="$emit('moveUp')" :disabled="selectedIndex === null || selectedIndex === 0" class="btn btn-secondary" :class="{'btn-disabled': selectedIndex === null || selectedIndex === 0}">
          <ArrowUpIcon class="w-5 h-5" />
        </button>
      </div>
      <div class="tooltip tooltip-bottom" data-tip="Move Down">
        <button @click="$emit('moveDown')" :disabled="selectedIndex === null || selectedIndex === patterns.length - 1" class="btn btn-secondary" :class="{'btn-disabled': selectedIndex === null || selectedIndex === patterns.length - 1}">
          <ArrowDownIcon class="w-5 h-5" />
        </button>
      </div>
      <div class="tooltip tooltip-bottom" data-tip="Delete Selected Pattern">
        <button @click="$emit('deletePattern')" :disabled="selectedIndex === null" class="btn btn-error" :class="{'btn-disabled': selectedIndex === null}">
          <TrashIcon class="w-5 h-5" />
        </button>
      </div>
    </div>
  </div>
</template>

<script>
import {
  PlusIcon,
  TrashIcon,
  PencilIcon,
  ArrowUpIcon,
  ArrowDownIcon,
  FolderIcon,
  DocumentDuplicateIcon
} from '@heroicons/vue/24/outline'
import logo from '../assets/texplitter.svg'

// `default` is the baseline ruleset the runtime loads with a zero-arg
// `Tokenizer()`. The backend refuses to rename or delete it; mirror
// that here so the Manage entries render as disabled rather than
// pretending to work and then bouncing off a 400 response.
const PROTECTED_RULESETS = new Set(['default'])

export default {
  props: {
    selectedIndex: { type: Number, default: null },
    patterns: { type: Array, default: () => [] },
    rulesets: { type: Array, default: () => [] },
    currentRuleset: { type: String, default: null }
  },
  emits: [
    'createPattern',
    'editPattern',
    'deletePattern',
    'moveUp',
    'moveDown',
    'selectRuleset',
    'createRuleset',
    'duplicateRuleset',
    'renameRuleset',
    'deleteRuleset'
  ],
  components: {
    PlusIcon,
    TrashIcon,
    PencilIcon,
    ArrowUpIcon,
    ArrowDownIcon,
    FolderIcon,
    DocumentDuplicateIcon
  },
  data() {
    return {
      logo,
      dropdownOpen: false,
      rulesetDropdownOpen: false
    }
  },
  computed: {
    canMutateCurrent() {
      return (
        this.currentRuleset !== null &&
        this.currentRuleset !== undefined &&
        !PROTECTED_RULESETS.has(this.currentRuleset)
      )
    }
  },
  methods: {
    toggleDropdown() {
      this.dropdownOpen = !this.dropdownOpen
      console.log('Dropdown toggled:', this.dropdownOpen)
    },
    handleOptionClick(position) {
      console.log(`Option clicked with position: ${position}`)
      this.emitCreatePattern(position)
      this.closeDropdown()
    },
    emitCreatePattern(position) {
      console.log(`Navbar emitting createPattern event with position: ${position}`)
      this.$emit('createPattern', position)
    },
    closeDropdown() {
      this.dropdownOpen = false
      console.log('Dropdown closed after option click')
    },
    toggleRulesetDropdown() {
      this.rulesetDropdownOpen = !this.rulesetDropdownOpen
    },
    closeRulesetDropdown() {
      this.rulesetDropdownOpen = false
    },
    handleRulesetClick(name) {
      console.log(`Navbar emitting selectRuleset event with name: ${name}`)
      this.$emit('selectRuleset', name)
      this.closeRulesetDropdown()
    },
    handleCreateRulesetClick() {
      this.$emit('createRuleset')
      this.closeRulesetDropdown()
    },
    handleDuplicateRulesetClick() {
      if (!this.currentRuleset) return
      this.$emit('duplicateRuleset')
      this.closeRulesetDropdown()
    },
    handleRenameRulesetClick() {
      if (!this.canMutateCurrent) return
      this.$emit('renameRuleset')
      this.closeRulesetDropdown()
    },
    handleDeleteRulesetClick() {
      if (!this.canMutateCurrent) return
      this.$emit('deleteRuleset')
      this.closeRulesetDropdown()
    }
  }
}
</script>

<style scoped>
.tooltip-bottom::before {
  content: attr(data-tip);
  position: absolute;
  bottom: -1.5rem; /* Adjust to place tooltip correctly */
  left: 50%;
  transform: translateX(-50%);
  background-color: rgba(0, 0, 0, 0.7);
  color: white;
  padding: 2px 5px; /* Minimal padding */
  border-radius: 4px;
  opacity: 0;
  white-space: nowrap;
  transition: opacity 0.2s;
  pointer-events: none;
  z-index: 50; /* Ensure the tooltip is above other elements */
  line-height: 1.2; /* Adjust line height to reduce padding */
}

.tooltip-bottom:hover::before {
  opacity: 1;
}

.disabled {
  opacity: 0.5;
}
</style>
