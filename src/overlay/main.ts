import { mount } from 'svelte'
import '../lib/styles/app.css'
import Overlay from './Overlay.svelte'

export default mount(Overlay, { target: document.getElementById('app')! })
