import { mount } from 'svelte'
import '../lib/styles/app.css'
import Flyout from './Flyout.svelte'

export default mount(Flyout, { target: document.getElementById('app')! })
