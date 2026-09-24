import { mount } from 'svelte'
import '../lib/styles/app.css'
import Notice from './Notice.svelte'

export default mount(Notice, { target: document.getElementById('app')! })
