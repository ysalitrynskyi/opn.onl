import { Link } from 'react-router-dom';
import { motion } from 'framer-motion';
import { Github, Heart, Users, Shield, Zap, Globe } from 'lucide-react';

const values = [
    {
        icon: <Shield className="h-6 w-6" />,
        title: "Privacy First",
        description: "We believe your data is yours. We don't track users across websites, sell data, or show ads."
    },
    {
        icon: <Zap className="h-6 w-6" />,
        title: "Performance",
        description: "Built with Rust and optional Redis caching. Optimized for fast, reliable redirects."
    },
    {
        icon: <Users className="h-6 w-6" />,
        title: "Open Source",
        description: "Transparency is key. Our entire codebase is open source, allowing anyone to verify, contribute, or self-host."
    },
    {
        icon: <Globe className="h-6 w-6" />,
        title: "Accessibility",
        description: "Great tools should be available to everyone. That's why opn.onl is free, without usage limits or premium tiers."
    }
];

export default function About() {
    return (
        <div className="pb-24">
            {/* Hero */}
            <section className="bg-gradient-to-b from-slate-50 to-white py-20">
                <div className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8">
                    <motion.div
                        initial={{ opacity: 0, y: 20 }}
                        animate={{ opacity: 1, y: 0 }}
                        className="text-center"
                    >
                        <h1 className="text-4xl sm:text-5xl font-extrabold text-slate-900 tracking-tight mb-6">
                            About opn.onl
                        </h1>
                        <p className="text-xl text-slate-600 max-w-2xl mx-auto leading-relaxed">
                            We're building the URL shortener we always wanted — fast, private, 
                            open source, and free for everyone.
                        </p>
                    </motion.div>
                </div>
            </section>

            {/* Story */}
            <section className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8 py-16">
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    whileInView={{ opacity: 1, y: 0 }}
                    viewport={{ once: true }}
                    className="prose prose-lg prose-slate mx-auto"
                >
                    <h2 className="text-3xl font-bold text-slate-900 mb-6">Our Story</h2>
                    <p>
                        opn.onl started as a simple idea: what if there was a URL shortener that 
                        didn't compromise on privacy? One that didn't track users across the web, 
                        sell data to advertisers, or hide features behind expensive paywalls?
                    </p>
                    <p>
                        Most URL shorteners treat users as the product. They collect vast amounts 
                        of data about who clicks what links, when, and from where — then monetize 
                        that information. We thought there had to be a better way.
                    </p>
                    <p>
                        So we built opn.onl. It's powered by Rust for incredible performance, 
                        React for a smooth user experience, and a commitment to privacy that's 
                        baked into everything we do. We collect only what's necessary, and we 
                        never sell your data.
                    </p>
                    <p>
                        Best of all, it's completely open source. You can inspect every line of 
                        code, contribute improvements, or deploy your own instance. No black boxes, 
                        no mysteries.
                    </p>
                </motion.div>
            </section>

            {/* Values */}
            <section className="bg-slate-50 py-20">
                <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <motion.div
                        initial={{ opacity: 0, y: 20 }}
                        whileInView={{ opacity: 1, y: 0 }}
                        viewport={{ once: true }}
                        className="text-center mb-16"
                    >
                        <h2 className="text-3xl font-bold text-slate-900 mb-4">Our Values</h2>
                        <p className="text-lg text-slate-600">
                            These principles guide everything we build.
                        </p>
                    </motion.div>

                    <div className="grid md:grid-cols-2 gap-8">
                        {values.map((value, index) => (
                            <motion.div
                                key={index}
                                initial={{ opacity: 0, y: 20 }}
                                whileInView={{ opacity: 1, y: 0 }}
                                viewport={{ once: true }}
                                transition={{ delay: index * 0.1 }}
                                className="bg-white p-8 rounded-2xl shadow-sm border border-slate-200"
                            >
                                <div className="h-12 w-12 bg-primary-100 rounded-xl flex items-center justify-center text-primary-600 mb-4">
                                    {value.icon}
                                </div>
                                <h3 className="text-xl font-bold text-slate-900 mb-2">{value.title}</h3>
                                <p className="text-slate-600">{value.description}</p>
                            </motion.div>
                        ))}
                    </div>
                </div>
            </section>

            {/* Open Source CTA */}
            <section className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8 py-20">
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    whileInView={{ opacity: 1, y: 0 }}
                    viewport={{ once: true }}
                    className="bg-slate-900 rounded-3xl p-8 sm:p-12 text-center"
                >
                    <Github className="h-12 w-12 text-white mx-auto mb-4" />
                    <h2 className="text-2xl sm:text-3xl font-bold text-white mb-4">
                        Open Source & Transparent
                    </h2>
                    <p className="text-slate-300 mb-8 max-w-xl mx-auto">
                        Our entire codebase is available on GitHub. Star us, fork us, 
                        contribute, or just poke around. We have nothing to hide.
                    </p>
                    <div className="flex flex-col sm:flex-row items-center justify-center gap-4">
                        <a
                            href="https://github.com/ysalitrynskyi/opn.onl"
                            target="_blank"
                            rel="noreferrer"
                            className="w-full sm:w-auto inline-flex items-center justify-center gap-2 bg-white text-slate-900 px-8 py-3 rounded-xl font-bold hover:bg-slate-100 transition-colors"
                        >
                            <Github className="h-5 w-5" />
                            View on GitHub
                        </a>
                        <Link
                            to="/contact"
                            className="w-full sm:w-auto inline-flex items-center justify-center gap-2 border border-slate-600 text-white px-8 py-3 rounded-xl font-bold hover:bg-slate-800 transition-colors"
                        >
                            <Heart className="h-5 w-5" />
                            Get in Touch
                        </Link>
                    </div>
                </motion.div>
            </section>
        </div>
    );
}
