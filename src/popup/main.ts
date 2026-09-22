import { mount } from 'svelte'
import '../lib/styles/app.css'
import Popup from './Popup.svelte'

export default mount(Popup, { target: document.getElementById('app')! })
