import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { Camera } from './camera'
import './styles.css'

createRoot(document.getElementById('root')!).render(<StrictMode><Camera /></StrictMode>)
