import { Link } from 'react-router-dom';
import { motion } from 'framer-motion';
import { Check, Sparkles, Building2, Heart } from 'lucide-react';

const plans = [
    {
        name: "Free",
        price: "$0",
        description: "Perfect for personal use and small projects",
        icon: <Heart className="h-6 w-6" />,
        features: [
            "Unlimited short links",
            "Custom aliases",
            "QR code generation",
            "Password protection",
            "Link expiration",
            "Basic analytics",
            "Bulk link creation",
            "CSV export",
            "API access"
        ],
        cta: "Get Started",
        ctaLink: "/register",
        highlighted: false
    },
    {
        name: "Pro",
        price: "$0",
        period: "forever",
        description: "All features, completely free. We're open source!",
        icon: <Sparkles className="h-6 w-6" />,
        features: [
            "Everything in Free",
            "Advanced analytics",
            "Detailed click data",
            "Device & browser tracking",
            "Geographic insights",
            "Referrer tracking",
            "Priority support",
            "Early access to features"
        ],
        cta: "Get Started",
        ctaLink: "/register",
        highlighted: true,
        badge: "Most Popular"
    },
    {
        name: "Self-Hosted",
        price: "$0",
        description: "Deploy on your own infrastructure",
        icon: <Building2 className="h-6 w-6" />,
        features: [
            "Full source code access",
            "Deploy anywhere",
            "Custom domain support",
            "Complete data ownership",
            "No usage limits",
            "Modify as needed",
            "Community support",
            "AGPL-3.0 licensed"
        ],
        cta: "View on GitHub",
        ctaLink: "https://github.com/ysalitrynskyi/opn.onl",
        external: true,
        highlighted: false
    }
];

const faqs = [
    {
        q: "Is opn.onl really free?",
        a: "Yes! opn.onl is 100% free and open source. We believe URL shortening should be accessible to everyone. There are no hidden fees, premium tiers, or usage limits."
    },
    {
        q: "How do you make money?",
        a: "We don't! opn.onl is a passion project built for the community. We're sustained by voluntary donations and the goodwill of contributors."
    },
    {
        q: "What's the catch?",
        a: "No catch. We're committed to privacy and transparency. Our code is open source, so you can verify exactly what we do with your data (spoiler: nothing shady)."
    },
    {
        q: "Can I self-host opn.onl?",
        a: "Absolutely! Our entire codebase is available on GitHub under the AGPL-3.0 license. You can deploy it on your own servers with your own domain."
    }
];

export default function Pricing() {
    return (
        <div className="pb-24">
            {/* Hero */}
            <section className="py-20 text-center">
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    className="max-w-4xl mx-auto px-4"
                >
                    <div className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-emerald-100 text-emerald-700 text-sm font-medium mb-6">
                        <Heart className="h-4 w-4" />
                        100% Free Forever
                    </div>
                    <h1 className="text-4xl sm:text-6xl font-extrabold text-slate-900 tracking-tight mb-6">
                        Simple pricing.<br />
                        <span className="text-transparent bg-clip-text bg-gradient-to-r from-emerald-600 to-teal-600">
                            Actually free.
                        </span>
                    </h1>
                    <p className="text-xl text-slate-600 max-w-2xl mx-auto">
                        No credit card required. No hidden fees. No premium tiers.
                        Just powerful link management for everyone.
                    </p>
                </motion.div>
            </section>

            {/* Pricing Cards */}
            <section className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                <div className="grid md:grid-cols-3 gap-8">
                    {plans.map((plan, index) => (
                        <motion.div
                            key={plan.name}
                            initial={{ opacity: 0, y: 20 }}
                            animate={{ opacity: 1, y: 0 }}
                            transition={{ delay: index * 0.1 }}
                            className={`relative rounded-2xl ${
                                plan.highlighted 
                                    ? 'bg-gradient-to-b from-primary-600 to-primary-700 text-white shadow-xl shadow-primary-500/20 scale-105' 
                                    : 'bg-white border border-slate-200 shadow-sm'
                            } p-8`}
                        >
                            {plan.badge && (
                                <div className="absolute -top-4 left-1/2 -translate-x-1/2">
                                    <span className="bg-amber-400 text-amber-900 text-xs font-bold px-3 py-1 rounded-full">
                                        {plan.badge}
                                    </span>
                                </div>
                            )}

                            <div className={`h-12 w-12 rounded-xl flex items-center justify-center mb-6 ${
                                plan.highlighted ? 'bg-white/20' : 'bg-slate-100'
                            }`}>
                                <span className={plan.highlighted ? 'text-white' : 'text-slate-600'}>
                                    {plan.icon}
                                </span>
                            </div>

                            <h3 className={`text-xl font-bold mb-2 ${plan.highlighted ? 'text-white' : 'text-slate-900'}`}>
                                {plan.name}
                            </h3>
                            
                            <div className="mb-4">
                                <span className={`text-4xl font-extrabold ${plan.highlighted ? 'text-white' : 'text-slate-900'}`}>
                                    {plan.price}
                                </span>
                                {plan.period && (
                                    <span className={`text-sm ml-2 ${plan.highlighted ? 'text-primary-200' : 'text-slate-500'}`}>
                                        {plan.period}
                                    </span>
                                )}
                            </div>

                            <p className={`mb-6 ${plan.highlighted ? 'text-primary-100' : 'text-slate-600'}`}>
                                {plan.description}
                            </p>

                            <ul className="space-y-3 mb-8">
                                {plan.features.map((feature, i) => (
                                    <li key={i} className="flex items-start gap-3">
                                        <Check className={`h-5 w-5 flex-shrink-0 ${
                                            plan.highlighted ? 'text-primary-200' : 'text-emerald-500'
                                        }`} />
                                        <span className={plan.highlighted ? 'text-primary-50' : 'text-slate-600'}>
                                            {feature}
                                        </span>
                                    </li>
                                ))}
                            </ul>

                            {plan.external ? (
                                <a
                                    href={plan.ctaLink}
                                    target="_blank"
                                    rel="noreferrer"
                                    className={`block text-center py-3 px-6 rounded-xl font-semibold transition-colors ${
                                        plan.highlighted
                                            ? 'bg-white text-primary-700 hover:bg-primary-50'
                                            : 'bg-slate-900 text-white hover:bg-slate-800'
                                    }`}
                                >
                                    {plan.cta}
                                </a>
                            ) : (
                                <Link
                                    to={plan.ctaLink}
                                    className={`block text-center py-3 px-6 rounded-xl font-semibold transition-colors ${
                                        plan.highlighted
                                            ? 'bg-white text-primary-700 hover:bg-primary-50'
                                            : 'bg-slate-900 text-white hover:bg-slate-800'
                                    }`}
                                >
                                    {plan.cta}
                                </Link>
                            )}
                        </motion.div>
                    ))}
                </div>
            </section>

            {/* FAQ */}
            <section className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8 py-24">
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    whileInView={{ opacity: 1, y: 0 }}
                    viewport={{ once: true }}
                    className="text-center mb-12"
                >
                    <h2 className="text-3xl font-bold text-slate-900 mb-4">
                        Frequently Asked Questions
                    </h2>
                    <p className="text-slate-600">
                        Still have questions? We've got answers.
                    </p>
                </motion.div>

                <div className="space-y-6">
                    {faqs.map((faq, index) => (
                        <motion.div
                            key={index}
                            initial={{ opacity: 0, y: 20 }}
                            whileInView={{ opacity: 1, y: 0 }}
                            viewport={{ once: true }}
                            transition={{ delay: index * 0.1 }}
                            className="bg-white p-6 rounded-xl border border-slate-200 shadow-sm"
                        >
                            <h3 className="text-lg font-bold text-slate-900 mb-2">{faq.q}</h3>
                            <p className="text-slate-600">{faq.a}</p>
                        </motion.div>
                    ))}
                </div>
            </section>

            {/* Support CTA */}
            <section className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8">
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    whileInView={{ opacity: 1, y: 0 }}
                    viewport={{ once: true }}
                    className="bg-gradient-to-r from-rose-500 to-pink-500 rounded-2xl p-8 sm:p-12 text-center text-white"
                >
                    <Heart className="h-12 w-12 mx-auto mb-4 fill-current" />
                    <h2 className="text-2xl sm:text-3xl font-bold mb-4">
                        Love opn.onl? Support the project!
                    </h2>
                    <p className="text-rose-100 mb-6 max-w-xl mx-auto">
                        opn.onl is built and maintained by volunteers. If you find it useful, 
                        consider starring us on GitHub or contributing to the project.
                    </p>
                    <div className="flex flex-col sm:flex-row items-center justify-center gap-4">
                        <a
                            href="https://github.com/ysalitrynskyi/opn.onl"
                            target="_blank"
                            rel="noreferrer"
                            className="w-full sm:w-auto bg-white text-rose-600 px-8 py-3 rounded-xl font-bold hover:bg-rose-50 transition-colors"
                        >
                            ⭐ Star on GitHub
                        </a>
                        <Link
                            to="/contact"
                            className="w-full sm:w-auto border-2 border-white/50 text-white px-8 py-3 rounded-xl font-bold hover:bg-white/10 transition-colors"
                        >
                            Get in Touch
                        </Link>
                    </div>
                </motion.div>
            </section>
        </div>
    );
}

