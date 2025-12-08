import { motion } from 'framer-motion';
import { Link } from 'react-router-dom';
import { Home, Search, ArrowLeft } from 'lucide-react';
import { Helmet } from 'react-helmet-async';

export default function NotFound() {
  return (
    <>
      <Helmet>
        <title>Page Not Found - opn.onl</title>
        <meta name="description" content="The page you're looking for doesn't exist or has been moved." />
        <meta name="robots" content="noindex, nofollow" />
      </Helmet>

      <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 flex items-center justify-center p-4">
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          className="max-w-lg w-full text-center"
        >
          {/* 404 Graphic */}
          <motion.div
            initial={{ scale: 0.8, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            transition={{ delay: 0.1, type: 'spring' }}
            className="mb-8"
          >
            <div className="relative inline-block">
              <span 
                className="text-[150px] md:text-[200px] font-black text-transparent bg-clip-text bg-gradient-to-b from-slate-600 to-slate-800 leading-none select-none"
                aria-hidden="true"
              >
                404
              </span>
              <motion.div
                animate={{ 
                  y: [0, -10, 0],
                  rotate: [0, -5, 5, 0]
                }}
                transition={{ 
                  duration: 3,
                  repeat: Infinity,
                  ease: 'easeInOut'
                }}
                className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2"
              >
                <Search className="w-16 h-16 md:w-20 md:h-20 text-blue-500" aria-hidden="true" />
              </motion.div>
            </div>
          </motion.div>

          {/* Text content */}
          <motion.div
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.2 }}
          >
            <h1 className="text-3xl md:text-4xl font-bold text-white mb-4">
              Page not found
            </h1>
            
            <p className="text-lg text-slate-400 mb-8 max-w-md mx-auto">
              The page you're looking for doesn't exist or has been moved. 
              Let's get you back on track.
            </p>
          </motion.div>

          {/* Actions */}
          <motion.div
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.3 }}
            className="flex flex-col sm:flex-row gap-4 justify-center"
          >
            <Link
              to="/"
              className="inline-flex items-center justify-center gap-2 px-6 py-3 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition-colors"
              aria-label="Go to homepage"
            >
              <Home className="w-5 h-5" aria-hidden="true" />
              Go to Homepage
            </Link>
            
            <button
              onClick={() => window.history.back()}
              className="inline-flex items-center justify-center gap-2 px-6 py-3 bg-slate-700 hover:bg-slate-600 text-white rounded-lg font-medium transition-colors"
              aria-label="Go back to previous page"
            >
              <ArrowLeft className="w-5 h-5" aria-hidden="true" />
              Go Back
            </button>
          </motion.div>

          {/* Helpful links */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ delay: 0.5 }}
            className="mt-12 pt-8 border-t border-slate-700/50"
          >
            <p className="text-slate-500 text-sm mb-4">Looking for something specific?</p>
            <nav aria-label="Helpful links" className="flex flex-wrap justify-center gap-4">
              <Link 
                to="/features" 
                className="text-blue-400 hover:text-blue-300 text-sm transition-colors"
              >
                Features
              </Link>
              <Link 
                to="/pricing" 
                className="text-blue-400 hover:text-blue-300 text-sm transition-colors"
              >
                Pricing
              </Link>
              <Link 
                to="/faq" 
                className="text-blue-400 hover:text-blue-300 text-sm transition-colors"
              >
                FAQ
              </Link>
              <Link 
                to="/contact" 
                className="text-blue-400 hover:text-blue-300 text-sm transition-colors"
              >
                Contact
              </Link>
            </nav>
          </motion.div>
        </motion.div>
      </div>
    </>
  );
}



