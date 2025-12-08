import { useState } from 'react';
import { Link } from 'react-router-dom';
import { motion, AnimatePresence } from 'framer-motion';
import { HelpCircle, ChevronDown, Search } from 'lucide-react';

const faqs = [
    {
        category: "Getting Started",
        questions: [
            {
                q: "Is opn.onl free to use?",
                a: "Yes! opn.onl is completely free to use for personal and commercial purposes. There are no hidden fees, premium tiers, or usage limits. We're an open source project committed to keeping URL shortening accessible to everyone."
            },
            {
                q: "Do I need an account to shorten links?",
                a: "While you can use opn.onl without an account, creating one unlocks powerful features like analytics tracking, custom aliases, password protection, link expiration, and the ability to manage all your links in one place."
            },
            {
                q: "How do I create a shortened link?",
                a: "Simply paste your long URL into the input field on our homepage or dashboard and click 'Shorten'. You can optionally add a custom alias, set an expiration date, or add password protection before creating the link."
            }
        ]
    },
    {
        category: "Features",
        questions: [
            {
                q: "Can I customize my short URL?",
                a: "Yes! Registered users can create custom aliases for their links (e.g., opn.onl/my-brand). Custom aliases must be between 3-20 characters and can only contain letters, numbers, and hyphens."
            },
            {
                q: "Do links expire?",
                a: "By default, links do not expire and will work indefinitely. However, you can optionally set an expiration date when creating or editing a link. Expired links will show an error message when accessed."
            },
            {
                q: "Can I password-protect my links?",
                a: "Absolutely! When creating or editing a link, you can add password protection. Anyone who clicks the link will be prompted to enter the password before being redirected to the destination."
            },
            {
                q: "What analytics are available?",
                a: "We provide detailed analytics including total clicks, click trends over time, geographic data, device types (mobile/desktop), browser information, and referrer sources. All data is displayed in easy-to-understand charts."
            },
            {
                q: "Can I generate QR codes for my links?",
                a: "Yes! Every link automatically gets a QR code that you can download and use on printed materials, presentations, or anywhere else. QR codes are available in PNG format."
            }
        ]
    },
    {
        category: "Security & Privacy",
        questions: [
            {
                q: "How secure is opn.onl?",
                a: "We take security seriously. All connections use HTTPS encryption, passwords are hashed using bcrypt, and we support modern passkey authentication for passwordless login. Our code is open source, so security researchers can verify our practices."
            },
            {
                q: "Do you track users who click my links?",
                a: "We collect basic analytics data (clicks, devices, referrers) to provide you with insights. However, we do NOT track users across websites, sell data to advertisers, or use any third-party tracking scripts."
            },
            {
                q: "What is passkey authentication?",
                a: "Passkeys are a modern, passwordless authentication method using your device's biometrics (fingerprint, face) or security key. They're more secure than passwords because they can't be phished or stolen. You can add a passkey in Settings."
            },
            {
                q: "Can I delete my data?",
                a: "Yes! You can delete individual links at any time from your dashboard. Link deletions are processed immediately. Account deletion is currently disabled on the hosted version, but if you need your account removed, please contact us at ys@opn.onl."
            }
        ]
    },
    {
        category: "Technical",
        questions: [
            {
                q: "Do you offer an API?",
                a: "Yes! All of opn.onl's functionality is available through our REST API. You can create links, retrieve analytics, and manage your links programmatically. Check the API documentation for details."
            },
            {
                q: "Can I self-host opn.onl?",
                a: "Absolutely! Our entire codebase is open source under the AGPL-3.0 license. You can clone the repository from GitHub and deploy your own instance with your own domain. The backend is built with Rust and the frontend with React."
            },
            {
                q: "What technology does opn.onl use?",
                a: "Our backend is built with Rust and the Axum web framework for performance. We use PostgreSQL for data storage with optional Redis caching for faster redirects. The frontend uses React with TypeScript and Tailwind CSS. We support WebAuthn for passkey authentication."
            },
            {
                q: "How fast are the redirects?",
                a: "We've optimized for speed using Rust and optional Redis caching. With Redis enabled, cached links redirect very quickly. Database lookups are efficient thanks to PostgreSQL indexing."
            }
        ]
    }
];

export default function Faq() {
    const [searchQuery, setSearchQuery] = useState('');
    const [openItems, setOpenItems] = useState<Set<string>>(new Set());

    const toggleItem = (id: string) => {
        setOpenItems(prev => {
            const next = new Set(prev);
            if (next.has(id)) {
                next.delete(id);
            } else {
                next.add(id);
            }
            return next;
        });
    };

    const filteredFaqs = searchQuery
        ? faqs.map(category => ({
            ...category,
            questions: category.questions.filter(
                q => q.q.toLowerCase().includes(searchQuery.toLowerCase()) ||
                     q.a.toLowerCase().includes(searchQuery.toLowerCase())
            )
        })).filter(category => category.questions.length > 0)
        : faqs;

    return (
        <div className="pb-24">
            {/* Hero */}
            <section className="bg-gradient-to-b from-slate-50 to-white py-16">
                <div className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8">
                    <motion.div
                        initial={{ opacity: 0, y: 20 }}
                        animate={{ opacity: 1, y: 0 }}
                        className="text-center"
                    >
                        <div className="h-16 w-16 bg-amber-100 rounded-2xl flex items-center justify-center mx-auto mb-6">
                            <HelpCircle className="h-8 w-8 text-amber-600" />
                        </div>
                        <h1 className="text-4xl font-extrabold text-slate-900 mb-4">
                            Frequently Asked Questions
                        </h1>
                        <p className="text-xl text-slate-600 max-w-2xl mx-auto mb-8">
                            Find answers to common questions about opn.onl
                        </p>

                        {/* Search */}
                        <div className="max-w-lg mx-auto relative">
                            <Search className="absolute left-4 top-1/2 -translate-y-1/2 h-5 w-5 text-slate-400" />
                            <input
                                type="text"
                                placeholder="Search questions..."
                                value={searchQuery}
                                onChange={e => setSearchQuery(e.target.value)}
                                className="w-full pl-12 pr-4 py-3 border border-slate-300 rounded-xl focus:border-primary-500 focus:ring-1 focus:ring-primary-500 shadow-sm"
                            />
                        </div>
                    </motion.div>
                </div>
            </section>

            {/* FAQ Sections */}
            <section className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8 py-12">
                {filteredFaqs.length === 0 ? (
                    <motion.div
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        className="text-center py-12"
                    >
                        <p className="text-slate-500 mb-4">No questions found matching "{searchQuery}"</p>
                        <button
                            onClick={() => setSearchQuery('')}
                            className="text-primary-600 font-medium hover:underline"
                        >
                            Clear search
                        </button>
                    </motion.div>
                ) : (
                    <div className="space-y-12">
                        {filteredFaqs.map((category, categoryIndex) => (
                            <motion.div
                                key={category.category}
                                initial={{ opacity: 0, y: 20 }}
                                animate={{ opacity: 1, y: 0 }}
                                transition={{ delay: categoryIndex * 0.1 }}
                            >
                                <h2 className="text-xl font-bold text-slate-900 mb-6">{category.category}</h2>
                                <div className="space-y-3">
                                    {category.questions.map((item, itemIndex) => {
                                        const itemId = `${categoryIndex}-${itemIndex}`;
                                        const isOpen = openItems.has(itemId);

                                        return (
                                            <div
                                                key={itemIndex}
                                                className="bg-white rounded-xl border border-slate-200 shadow-sm overflow-hidden"
                                            >
                                                <button
                                                    onClick={() => toggleItem(itemId)}
                                                    className="w-full px-6 py-4 flex items-center justify-between text-left hover:bg-slate-50 transition-colors"
                                                >
                                                    <span className="font-medium text-slate-900 pr-4">{item.q}</span>
                                                    <ChevronDown 
                                                        className={`h-5 w-5 text-slate-400 flex-shrink-0 transition-transform ${
                                                            isOpen ? 'rotate-180' : ''
                                                        }`} 
                                                    />
                                                </button>
                                                <AnimatePresence>
                                                    {isOpen && (
                                                        <motion.div
                                                            initial={{ height: 0, opacity: 0 }}
                                                            animate={{ height: 'auto', opacity: 1 }}
                                                            exit={{ height: 0, opacity: 0 }}
                                                            transition={{ duration: 0.2 }}
                                                            className="overflow-hidden"
                                                        >
                                                            <div className="px-6 pb-4 text-slate-600 leading-relaxed border-t border-slate-100 pt-4">
                                                                {item.a}
                                                            </div>
                                                        </motion.div>
                                                    )}
                                                </AnimatePresence>
                                            </div>
                                        );
                                    })}
                                </div>
                            </motion.div>
                        ))}
                    </div>
                )}

                {/* Still have questions */}
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    whileInView={{ opacity: 1, y: 0 }}
                    viewport={{ once: true }}
                    className="mt-16 bg-slate-50 rounded-2xl p-8 text-center"
                >
                    <h3 className="text-xl font-bold text-slate-900 mb-2">Still have questions?</h3>
                    <p className="text-slate-600 mb-6">
                        Can't find what you're looking for? We're here to help.
                    </p>
                    <Link
                        to="/contact"
                        className="inline-flex items-center gap-2 bg-primary-600 text-white px-6 py-3 rounded-xl font-semibold hover:bg-primary-700 transition-colors"
                    >
                        Contact Support
                    </Link>
                </motion.div>
            </section>
        </div>
    );
}
