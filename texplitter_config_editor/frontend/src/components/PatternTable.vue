<template>
  <div class="table-container h-full overflow-auto w-full">
    <table class="table w-full">
      <thead>
        <tr>
          <th class="sticky top-0 bg-white z-20">Type</th>
          <th class="sticky top-0 bg-white z-20">Class</th>
          <th class="sticky top-0 bg-white z-20">Description</th>
          <th class="sticky top-0 bg-white z-20">Regex</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="(pattern, index) in patterns" :key="index" class="hover:bg-gray-100 cursor-pointer" @click="$emit('selectRow', index)" :class="{'bg-blue-100': selectedIndex === index}">
          <td class="align-top">{{ pattern.type }}</td>
          <td class="align-top">{{ pattern.class }}</td>
          <td class="align-top">{{ pattern.description }}</td>
          <td class="align-top">
            <pre class="regex-box">
              <code class="language-plaintext">{{ pattern.regex }}</code>
            </pre>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<script>
import hljs from 'highlight.js'
import 'highlight.js/styles/default.css'

export default {
  props: ['patterns', 'selectedIndex'],
  updated() {
    this.$nextTick(() => {
      document.querySelectorAll('pre code').forEach((block) => {
        hljs.highlightElement(block);
      });
    });
  }
}
</script>

<style scoped>
.table-container {
  height: 100%; /* Expand to fill the container */
  overflow-y: auto; /* Enable vertical scrolling */
  width: 100%; /* Ensure full width */
}
thead th {
  position: sticky;
  top: 0;
  background-color: white; /* Ensure the background is white */
  z-index: 30; /* Ensure the header is above other elements */
}
tbody tr:nth-child(odd) {
  background-color: #f9f9f9; /* Set background color for odd rows for striping */
}
tbody tr:nth-child(even) {
  background-color: #ffffff; /* Set background color for even rows */
}
tbody tr.bg-blue-100 {
  background-color: #cce5ff !important; /* Custom highlight color for selected row */
}
td.align-top {
  vertical-align: top; /* Align items at the top */
}
.regex-box {
  white-space: pre-wrap; /* Ensure the text wraps within the box */
  background-color: #f7f7f7; /* Light background for code readability */
  padding: 10px; /* Add some padding for better look */
  border-radius: 5px; /* Rounded corners */
  border: 1px solid #ddd; /* Light border */
  overflow-x: auto; /* Ensure the content doesn't overflow the container */
}
.tooltip {
  position: relative;
  display: inline-block;
  cursor: pointer;
}

.tooltip::before {
  content: attr(data-tip);
  position: absolute;
  bottom: -100%; /* Adjusted to show the tooltip below */
  left: 50%;
  transform: translateX(-50%);
  background-color: rgba(0, 0, 0, 0.7);
  color: white;
  padding: 5px 10px;
  border-radius: 4px;
  opacity: 0;
  white-space: nowrap;
  transition: opacity 0.2s;
  pointer-events: none;
  margin-top: 5px;
  z-index: 40; /* Ensure the tooltip is above other elements */
}

.tooltip:hover::before {
  opacity: 1;
}
</style>
