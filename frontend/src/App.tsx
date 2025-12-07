import { Routes, Route } from 'react-router-dom';
import { HelmetProvider } from 'react-helmet-async';
import { Suspense, lazy } from 'react';
import ErrorBoundary from './components/ErrorBoundary';
import Layout from './components/Layout';
import Home from './pages/Home';
import Login from './pages/Login';
import Register from './pages/Register';
import NotFound from './pages/NotFound';

// Lazy load heavier pages for better initial load
const Dashboard = lazy(() => import('./pages/Dashboard'));
const Analytics = lazy(() => import('./pages/Analytics'));
const Settings = lazy(() => import('./pages/Settings'));
const PasswordPrompt = lazy(() => import('./pages/PasswordPrompt'));
const Features = lazy(() => import('./pages/Features'));
const Pricing = lazy(() => import('./pages/Pricing'));
const About = lazy(() => import('./pages/About'));
const Privacy = lazy(() => import('./pages/Privacy'));
const Terms = lazy(() => import('./pages/Terms'));
const Contact = lazy(() => import('./pages/Contact'));
const Faq = lazy(() => import('./pages/Faq'));
const VerifyEmail = lazy(() => import('./pages/VerifyEmail'));
const ForgotPassword = lazy(() => import('./pages/ForgotPassword'));
const ResetPassword = lazy(() => import('./pages/ResetPassword'));
const Admin = lazy(() => import('./pages/Admin'));

// Loading fallback component
function PageLoader() {
  return (
    <div className="min-h-[60vh] flex items-center justify-center">
      <div className="flex flex-col items-center gap-4">
        <div className="w-8 h-8 border-4 border-primary-200 border-t-primary-600 rounded-full animate-spin" />
        <p className="text-slate-500 text-sm">Loading...</p>
      </div>
    </div>
  );
}

function App() {
  return (
    <HelmetProvider>
      <ErrorBoundary>
        <Suspense fallback={<PageLoader />}>
          <Routes>
            <Route path="/" element={<Layout />}>
              <Route index element={<Home />} />
              <Route path="login" element={<Login />} />
              <Route path="register" element={<Register />} />
              <Route path="dashboard" element={<Dashboard />} />
              <Route path="analytics/:id" element={<Analytics />} />
              <Route path="settings" element={<Settings />} />
              <Route path="password/:code" element={<PasswordPrompt />} />
              <Route path="features" element={<Features />} />
              <Route path="pricing" element={<Pricing />} />
              <Route path="about" element={<About />} />
              <Route path="privacy" element={<Privacy />} />
              <Route path="terms" element={<Terms />} />
              <Route path="contact" element={<Contact />} />
              <Route path="faq" element={<Faq />} />
              <Route path="verify-email" element={<VerifyEmail />} />
              <Route path="forgot-password" element={<ForgotPassword />} />
              <Route path="reset-password" element={<ResetPassword />} />
              <Route path="admin" element={<Admin />} />
              <Route path="*" element={<NotFound />} />
            </Route>
          </Routes>
        </Suspense>
      </ErrorBoundary>
    </HelmetProvider>
  );
}

export default App;
