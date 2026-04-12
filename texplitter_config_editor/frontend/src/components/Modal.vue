<template>
    <div class="modal modal-open" v-if="show">
      <div class="modal-box" :style="{ width: width }">
        <h3 class="font-bold text-lg" v-if="title">{{ title }}</h3>
        <div class="py-4">
          <slot></slot>
        </div>
        <div class="modal-action">
          <button @click="handleOk" class="btn btn-primary">{{ okText }}</button>
          <button @click="handleCancel" class="btn">{{ cancelText }}</button>
        </div>
      </div>
    </div>
  </template>
  
  <script>
  export default {
    props: {
      show: {
        type: Boolean,
        required: true
      },
      title: {
        type: String,
        default: ''
      },
      okText: {
        type: String,
        default: 'OK'
      },
      cancelText: {
        type: String,
        default: 'Cancel'
      },
      width: {
        type: String,
        default: 'auto'
      }
    },
    methods: {
      handleOk() {
        this.$emit('ok')
      },
      handleCancel() {
        this.$emit('cancel')
      }
    }
  }
  </script>
  
  <style scoped>
  .modal-box {
    max-width: 100%; /* Ensure it doesn't exceed the viewport width */
  }
  </style>
  