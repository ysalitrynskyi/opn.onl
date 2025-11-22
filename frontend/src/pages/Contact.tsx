import { useState } from 'react';
import { motion } from 'framer-motion';
import { Mail, MessageSquare, Github, Send, CheckCircle, AlertCircle } from 'lucide-react';
import { API_ENDPOINTS } from '../config/api';
import SEO from '../components/SEO';

export default function Contact() {
    const [formData, setFormData] = useState({
        name: '',
        email: '',
        subject: '',
        message: ''
    });
    const [status, setStatus] = useState<'idle' | 'sending' | 'success' | 'error'>('idle');
    const [errorMessage, setErrorMessage] = useState('');

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        setStatus('sending');
        setErrorMessage('');
        
        try {
            const res = await fetch(API_ENDPOINTS.contact, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(formData),
            });

            const data = await res.json();

            if (!res.ok || !data.success) {
                throw new Error(data.message || 'Failed to send message');
            }

            setStatus('success');
            setFormData({ name: '', email: '', subject: '', message: '' });
        } catch (err: unknown) {
            setStatus('error');
            setErrorMessage(err instanceof Error ? err.message : 'Failed to send message');
        }
    };

    const handleChange = (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>) => {
        setFormData(prev => ({
            ...prev,
            [e.target.name]: e.target.value
        }));
    };

    return (
        <>
            <SEO
                title="Contact Us"
                description="Get in touch with the opn.onl team. We're here to help with questions, feedback, or support."
                url="/contact"
            />
            <div className="pb-24">
                {/* Hero */}
                <section className="bg-gradient-to-b from-slate-50 to-white py-16">
                    <div className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8">
                        <motion.div
                            initial={{ opacity: 0, y: 20 }}
                            animate={{ opacity: 1, y: 0 }}
                            className="text-center"
                        >
                            <div className="h-16 w-16 bg-primary-100 rounded-2xl flex items-center justify-center mx-auto mb-6">
                                <MessageSquare className="h-8 w-8 text-primary-600" aria-hidden="true" />
                            </div>
                            <h1 className="text-4xl font-extrabold text-slate-900 mb-4">Contact Us</h1>
                            <p className="text-xl text-slate-600 max-w-2xl mx-auto">
                                Have questions, feedback, or need help? We'd love to hear from you.
                            </p>
                        </motion.div>
                    </div>
                </section>

                {/* Contact Options */}
                <section className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-12">
                    <div className="grid md:grid-cols-3 gap-6 mb-16">
                        <motion.a
                            href="mailto:support@opn.onl"
                            initial={{ opacity: 0, y: 20 }}
                            animate={{ opacity: 1, y: 0 }}
                            transition={{ delay: 0.1 }}
                            className="bg-white p-6 rounded-2xl border border-slate-200 shadow-sm hover:shadow-md transition-shadow text-center group"
                        >
                            <div className="h-12 w-12 bg-primary-100 rounded-xl flex items-center justify-center mx-auto mb-4 group-hover:scale-110 transition-transform">
                                <Mail className="h-6 w-6 text-primary-600" aria-hidden="true" />
                            </div>
                            <h3 className="font-bold text-slate-900 mb-2">Email Support</h3>
                            <p className="text-slate-500 text-sm mb-3">For general inquiries and help</p>
                            <span className="text-primary-600 font-medium">support@opn.onl</span>
                        </motion.a>

                        <motion.a
                            href="https://github.com/salitrynskyi/opn.onl/issues"
                            target="_blank"
                            rel="noreferrer"
                            initial={{ opacity: 0, y: 20 }}
                            animate={{ opacity: 1, y: 0 }}
                            transition={{ delay: 0.2 }}
                            className="bg-white p-6 rounded-2xl border border-slate-200 shadow-sm hover:shadow-md transition-shadow text-center group"
                        >
                            <div className="h-12 w-12 bg-slate-100 rounded-xl flex items-center justify-center mx-auto mb-4 group-hover:scale-110 transition-transform">
                                <Github className="h-6 w-6 text-slate-700" aria-hidden="true" />
                            </div>
                            <h3 className="font-bold text-slate-900 mb-2">GitHub Issues</h3>
                            <p className="text-slate-500 text-sm mb-3">Report bugs or request features</p>
                            <span className="text-primary-600 font-medium">Open an Issue →</span>
                        </motion.a>

                        <motion.div
                            initial={{ opacity: 0, y: 20 }}
                            animate={{ opacity: 1, y: 0 }}
                            transition={{ delay: 0.3 }}
                            className="bg-white p-6 rounded-2xl border border-slate-200 shadow-sm text-center"
                        >
                            <div className="h-12 w-12 bg-emerald-100 rounded-xl flex items-center justify-center mx-auto mb-4">
                                <MessageSquare className="h-6 w-6 text-emerald-600" aria-hidden="true" />
                            </div>
                            <h3 className="font-bold text-slate-900 mb-2">Response Time</h3>
                            <p className="text-slate-500 text-sm mb-3">We typically respond within</p>
                            <span className="text-emerald-600 font-medium">24-48 hours</span>
                        </motion.div>
                    </div>

                    {/* Contact Form */}
                    <motion.div
                        initial={{ opacity: 0, y: 20 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ delay: 0.4 }}
                        className="max-w-2xl mx-auto"
                    >
                        <div className="bg-white p-8 rounded-2xl border border-slate-200 shadow-sm">
                            <h2 className="text-2xl font-bold text-slate-900 mb-6">Send us a message</h2>

                            {status === 'success' ? (
                                <motion.div
                                    initial={{ opacity: 0, scale: 0.95 }}
                                    animate={{ opacity: 1, scale: 1 }}
                                    className="text-center py-12"
                                >
                                    <div className="h-16 w-16 bg-emerald-100 rounded-full flex items-center justify-center mx-auto mb-4">
                                        <CheckCircle className="h-8 w-8 text-emerald-600" aria-hidden="true" />
                                    </div>
                                    <h3 className="text-xl font-bold text-slate-900 mb-2">Message Sent!</h3>
                                    <p className="text-slate-600 mb-6">
                                        Thank you for reaching out. We'll get back to you as soon as possible.
                                    </p>
                                    <button
                                        onClick={() => setStatus('idle')}
                                        className="text-primary-600 font-medium hover:underline"
                                    >
                                        Send another message
                                    </button>
                                </motion.div>
                            ) : (
                                <form onSubmit={handleSubmit} className="space-y-5">
                                    <div className="grid sm:grid-cols-2 gap-5">
                                        <div>
                                            <label htmlFor="name" className="block text-sm font-medium text-slate-700 mb-1">
                                                Your Name
                                            </label>
                                            <input
                                                type="text"
                                                id="name"
                                                name="name"
                                                required
                                                aria-required="true"
                                                value={formData.name}
                                                onChange={handleChange}
                                                className="w-full px-4 py-2.5 border border-slate-300 rounded-lg bg-white text-slate-900 placeholder:text-slate-400 focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                                                placeholder="John Doe"
                                            />
                                        </div>
                                        <div>
                                            <label htmlFor="email" className="block text-sm font-medium text-slate-700 mb-1">
                                                Email Address
                                            </label>
                                            <input
                                                type="email"
                                                id="email"
                                                name="email"
                                                required
                                                aria-required="true"
                                                value={formData.email}
                                                onChange={handleChange}
                                                className="w-full px-4 py-2.5 border border-slate-300 rounded-lg bg-white text-slate-900 placeholder:text-slate-400 focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                                                placeholder="john@example.com"
                                            />
                                        </div>
                                    </div>

                                    <div>
                                        <label htmlFor="subject" className="block text-sm font-medium text-slate-700 mb-1">
                                            Subject
                                        </label>
                                        <select
                                            id="subject"
                                            name="subject"
                                            required
                                            aria-required="true"
                                            value={formData.subject}
                                            onChange={handleChange}
                                            className="w-full px-4 py-2.5 border border-slate-300 rounded-lg bg-white text-slate-900 focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                                        >
                                            <option value="">Select a topic...</option>
                                            <option value="general">General Inquiry</option>
                                            <option value="support">Technical Support</option>
                                            <option value="feedback">Feedback & Suggestions</option>
                                            <option value="bug">Bug Report</option>
                                            <option value="business">Business / Partnership</option>
                                            <option value="other">Other</option>
                                        </select>
                                    </div>

                                    <div>
                                        <label htmlFor="message" className="block text-sm font-medium text-slate-700 mb-1">
                                            Message
                                        </label>
                                        <textarea
                                            id="message"
                                            name="message"
                                            required
                                            aria-required="true"
                                            rows={5}
                                            minLength={10}
                                            maxLength={5000}
                                            value={formData.message}
                                            onChange={handleChange}
                                            className="w-full px-4 py-2.5 border border-slate-300 rounded-lg bg-white text-slate-900 placeholder:text-slate-400 focus:border-primary-500 focus:ring-1 focus:ring-primary-500 resize-none"
                                            placeholder="How can we help you?"
                                        />
                                    </div>

                                    {status === 'error' && (
                                        <motion.div 
                                            initial={{ opacity: 0, y: -10 }}
                                            animate={{ opacity: 1, y: 0 }}
                                            className="flex items-center gap-2 p-4 bg-red-50 border border-red-200 rounded-lg text-red-700"
                                            role="alert"
                                        >
                                            <AlertCircle className="h-5 w-5 flex-shrink-0" aria-hidden="true" />
                                            <p className="text-sm">{errorMessage || 'Something went wrong. Please try again or email us directly.'}</p>
                                        </motion.div>
                                    )}

                                    <button
                                        type="submit"
                                        disabled={status === 'sending'}
                                        className="w-full bg-primary-600 text-white py-3 px-4 rounded-lg font-semibold hover:bg-primary-700 transition-colors disabled:opacity-70 flex items-center justify-center gap-2"
                                    >
                                        {status === 'sending' ? (
                                            <div className="h-5 w-5 border-2 border-white/30 border-t-white rounded-full animate-spin" aria-label="Sending..." />
                                        ) : (
                                            <>
                                                <Send className="h-4 w-4" aria-hidden="true" />
                                                Send Message
                                            </>
                                        )}
                                    </button>
                                </form>
                            )}
                        </div>
                    </motion.div>
                </section>
            </div>
        </>
    );
}
