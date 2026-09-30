import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@fontsource-variable/bricolage-grotesque/wght.css'
import '@fontsource-variable/hanken-grotesk/wght.css'
import '@fontsource-variable/jetbrains-mono/wght.css'
import './index.css'
import App from './App.tsx'
import { dropPrerenderedHeadTags } from './utils/prerenderedHead'

import { BrowserRouter } from 'react-router-dom'

// Before the first render: the SEO component re-renders these for the route
// that is actually open.
dropPrerenderedHeadTags()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <BrowserRouter>
      <App />
    </BrowserRouter>
  </StrictMode>,
)
