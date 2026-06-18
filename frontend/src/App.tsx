import { Routes, Route, useLocation } from 'react-router-dom';
import { HelmetProvider } from 'react-helmet-async';
import { Suspense, lazy, useEffect } from 'react';
import ErrorBoundary from './components/ErrorBoundary';
import Layout from './components/Layout';
import { ToastContainer } from './components/Toast';
import Home from './pages/Home';
import Login from './pages/Login';
import Register from './pages/Register';
import NotFound from './pages/NotFound';
// Marketing pages — eager imports so they prerender to full static HTML (SEO).
// Lazy routes only render the Layout shell during prerender; eager ones don't.
import Features from './pages/Features';
import Pricing from './pages/Pricing';
import About from './pages/About';
import Privacy from './pages/Privacy';
import Terms from './pages/Terms';
import Contact from './pages/Contact';
import Faq from './pages/Faq';
import Docs from './pages/Docs';
import Developers from './pages/Developers';

// Scroll to top on route change
function ScrollToTop() {
  const { pathname } = useLocation();
  
  useEffect(() => {
    window.scrollTo(0, 0);
  }, [pathname]);
  
  return null;
}

// Lazy-load heavier auth / app / dynamic pages (not prerendered)
const Dashboard = lazy(() => import('./pages/Dashboard'));
const Analytics = lazy(() => import('./pages/Analytics'));
const Settings = lazy(() => import('./pages/Settings'));
const PasswordPrompt = lazy(() => import('./pages/PasswordPrompt'));
const VerifyEmail = lazy(() => import('./pages/VerifyEmail'));
const Preview = lazy(() => import('./pages/Preview'));
const ForgotPassword = lazy(() => import('./pages/ForgotPassword'));
const ResetPassword = lazy(() => import('./pages/ResetPassword'));
const Admin = lazy(() => import('./pages/Admin'));
const Redirect = lazy(() => import('./pages/Redirect'));
const Bio = lazy(() => import('./pages/Bio'));

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
        <ScrollToTop />
        <ToastContainer />
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
              <Route path="docs" element={<Docs />} />
              <Route path="developers" element={<Developers />} />
              <Route path="verify-email" element={<VerifyEmail />} />
              <Route path="forgot-password" element={<ForgotPassword />} />
              <Route path="reset-password" element={<ResetPassword />} />
              <Route path="admin" element={<Admin />} />
              <Route path="@:username" element={<Bio />} />
              {/* Safe-link interstitial: the backend 302s opted-in links here so
                  the SPA can show the "you're leaving" screen before continuing. */}
              <Route path="r/:code" element={<Redirect />} />
              <Route path=":code+" element={<Preview />} />
              <Route path=":code" element={<Redirect />} />
              <Route path="*" element={<NotFound />} />
            </Route>
          </Routes>
        </Suspense>
      </ErrorBoundary>
    </HelmetProvider>
  );
}

export default App;
