import { useState, useEffect } from 'react'
import axios from 'axios'
import {
  Brain, TrendingUp, Users, Clock, CheckSquare, Package, Home,
  Plus, Trash2, ChevronRight, AlertCircle, Sparkles, BarChart3,
  ArrowUpRight, ArrowDownRight, Minus
} from 'lucide-react'

const api = axios.create({
  baseURL: `${import.meta.env.VITE_API_URL || ''}/api/v1`,
  headers: { 'Content-Type': 'application/json' },
})
api.interceptors.request.use(cfg => {
  const token = localStorage.getItem('access_token')
  if (token) cfg.headers.Authorization = `Bearer ${token}`
  return cfg
})

interface DimensionScore { score: number; label: string; trend: string; note: string }
interface HappinessReport {
  family_id: string; total_score: number; trend: string; generated_at: string
  dimensions: { people: DimensionScore; time: DimensionScore; tasks: DimensionScore; things: DimensionScore; spaces: DimensionScore }
  suggestions: { category: string; priority: string; title: string; description: string }[]
}
interface Decision {
  id: string; title: string; description?: string
  options: { name: string; pros: string[]; cons: string[] }[]
  recommendation?: string; status: string; created_at: string
}

function ScoreRing({ score, size = 80 }: { score: number; size?: number }) {
  const r = size / 2 - 8
  const circ = 2 * Math.PI * r
  const dash = (score / 100) * circ
  const color = score >= 80 ? '#22c55e' : score >= 60 ? '#eab308' : '#ef4444'
  return (
    <svg width={size} height={size} style={{ transform: 'rotate(-90deg)' }}>
      <circle cx={size/2} cy={size/2} r={r} fill="none" stroke="#f3f4f6" strokeWidth="8" />
      <circle cx={size/2} cy={size/2} r={r} fill="none" stroke={color} strokeWidth="8"
        strokeDasharray={`${dash} ${circ}`} strokeLinecap="round" />
      <text x={size/2} y={size/2 + 1} textAnchor="middle" dominantBaseline="middle"
        style={{ transform: 'rotate(90deg)', transformOrigin: `${size/2}px ${size/2}px`, fontSize: size * 0.22, fontWeight: 700, fill: '#111827' }}>
        {score.toFixed(0)}
      </text>
    </svg>
  )
}

function TrendIcon({ trend }: { trend: string }) {
  if (trend === '上升') return <ArrowUpRight className="w-3.5 h-3.5 text-green-500" />
  if (trend === '下降') return <ArrowDownRight className="w-3.5 h-3.5 text-red-500" />
  return <Minus className="w-3.5 h-3.5 text-gray-400" />
}

const DIM_ICONS: Record<string, React.ReactNode> = {
  people: <Users className="w-4 h-4" />,
  time: <Clock className="w-4 h-4" />,
  tasks: <CheckSquare className="w-4 h-4" />,
  things: <Package className="w-4 h-4" />,
  spaces: <Home className="w-4 h-4" />,
}

type Tab = 'happiness' | 'decisions'

export default function App() {
  const [tab, setTab] = useState<Tab>('happiness')
  const [report, setReport] = useState<HappinessReport | null>(null)
  const [decisions, setDecisions] = useState<Decision[]>([])
  const [loading, setLoading] = useState(false)
  const [showCreate, setShowCreate] = useState(false)
  const [newDecision, setNewDecision] = useState({ title: '', description: '', options: [{ name: '', pros: '', cons: '' }] })

  const token = localStorage.getItem('access_token')
  const authed = !!token

  useEffect(() => {
    if (!authed) return
    if (tab === 'happiness') fetchReport()
    if (tab === 'decisions') fetchDecisions()
  }, [tab, authed])

  const fetchReport = async () => {
    setLoading(true)
    try {
      const { data } = await api.get<HappinessReport>('/happiness/report')
      setReport(data)
    } catch { setReport(null) }
    finally { setLoading(false) }
  }

  const fetchDecisions = async () => {
    setLoading(true)
    try {
      const { data } = await api.get<Decision[]>('/decisions')
      setDecisions(data)
    } catch { setDecisions([]) }
    finally { setLoading(false) }
  }

  const submitDecision = async () => {
    const options = newDecision.options
      .filter(o => o.name)
      .map(o => ({
        name: o.name,
        pros: o.pros.split('\n').filter(Boolean),
        cons: o.cons.split('\n').filter(Boolean),
      }))
    await api.post('/decisions', { title: newDecision.title, description: newDecision.description || undefined, options })
    setShowCreate(false)
    setNewDecision({ title: '', description: '', options: [{ name: '', pros: '', cons: '' }] })
    fetchDecisions()
  }

  if (!authed) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-gray-50">
        <div className="card text-center p-8 max-w-sm">
          <Brain className="w-12 h-12 text-primary-500 mx-auto mb-4" />
          <h2 className="text-xl font-bold text-gray-900 mb-2">JiaBu 决策助手</h2>
          <p className="text-gray-500 text-sm mb-6">请先登录 HamR 账号中心，然后返回此页面。</p>
          <a href={import.meta.env.VITE_ACCOUNT_URL || 'http://account.hamr.store/login'}
            className="btn-primary inline-block">前往登录</a>
        </div>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-gray-50">
      <header className="bg-white border-b border-gray-200">
        <div className="max-w-4xl mx-auto px-4 py-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 bg-primary-600 rounded-lg flex items-center justify-center">
              <Brain className="w-4 h-4 text-white" />
            </div>
            <div>
              <h1 className="font-bold text-gray-900">JiaBu 决策助手</h1>
              <p className="text-xs text-gray-500">家庭智能决策引擎</p>
            </div>
          </div>
          <div className="flex gap-1">
            {(['happiness', 'decisions'] as Tab[]).map(t => (
              <button key={t} onClick={() => setTab(t)}
                className={`px-4 py-1.5 rounded-lg text-sm font-medium transition-colors ${tab === t ? 'bg-primary-50 text-primary-700' : 'text-gray-500 hover:text-gray-700'}`}>
                {t === 'happiness' ? '幸福报告' : '决策辅助'}
              </button>
            ))}
          </div>
        </div>
      </header>

      <main className="max-w-4xl mx-auto px-4 py-8">
        {tab === 'happiness' && (
          loading ? <div className="text-center py-16 text-gray-400">生成报告中...</div>
          : report ? (
            <div className="space-y-6">
              <div className="card flex items-center gap-6">
                <ScoreRing score={report.total_score} size={100} />
                <div>
                  <div className="text-sm text-gray-500">家庭幸福指数</div>
                  <div className="text-3xl font-bold text-gray-900">{report.total_score}</div>
                  <div className="flex items-center gap-1 mt-1">
                    <TrendIcon trend={report.trend} />
                    <span className="text-sm text-gray-500">趋势{report.trend}</span>
                  </div>
                </div>
              </div>

              <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
                {Object.entries(report.dimensions).map(([key, dim]) => (
                  <div key={key} className="card">
                    <div className="flex items-center justify-between mb-2">
                      <div className="flex items-center gap-2 text-gray-500 text-sm">
                        {DIM_ICONS[key]}
                        <span>{dim.label}</span>
                      </div>
                      <TrendIcon trend={dim.trend} />
                    </div>
                    <div className="text-2xl font-bold text-gray-900">{dim.score.toFixed(0)}</div>
                    <div className="w-full bg-gray-100 rounded-full h-1.5 mt-2">
                      <div className="h-1.5 rounded-full transition-all"
                        style={{ width: `${dim.score}%`, backgroundColor: dim.score >= 80 ? '#22c55e' : dim.score >= 60 ? '#eab308' : '#ef4444' }} />
                    </div>
                    <div className="text-xs text-gray-400 mt-2 line-clamp-2">{dim.note}</div>
                  </div>
                ))}
              </div>

              {report.suggestions.length > 0 && (
                <div className="card">
                  <h2 className="font-semibold text-gray-900 mb-4 flex items-center gap-2">
                    <Sparkles className="w-4 h-4 text-primary-500" />智能建议
                  </h2>
                  <div className="space-y-3">
                    {report.suggestions.map((s, i) => (
                      <div key={i} className="flex items-start gap-3 p-3 bg-gray-50 rounded-lg">
                        <span className={`shrink-0 text-xs px-2 py-0.5 rounded-full font-medium ${
                          s.priority === 'high' ? 'bg-red-100 text-red-700' :
                          s.priority === 'medium' ? 'bg-yellow-100 text-yellow-700' :
                          'bg-gray-100 text-gray-600'
                        }`}>{s.category}</span>
                        <div>
                          <div className="text-sm font-medium text-gray-900">{s.title}</div>
                          <div className="text-xs text-gray-500 mt-0.5">{s.description}</div>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          ) : (
            <div className="card text-center py-12">
              <BarChart3 className="w-12 h-12 text-gray-300 mx-auto mb-3" />
              <p className="text-gray-500">无法获取幸福报告</p>
              <p className="text-xs text-gray-400 mt-1">请确保已在管家应用中添加家庭数据</p>
              <button onClick={fetchReport} className="btn-primary mt-4">重试</button>
            </div>
          )
        )}

        {tab === 'decisions' && (
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <h2 className="text-lg font-bold text-gray-900">决策列表</h2>
              <button onClick={() => setShowCreate(true)} className="btn-primary flex items-center gap-1">
                <Plus className="w-4 h-4" />新建决策
              </button>
            </div>

            {showCreate && (
              <div className="card border-primary-200">
                <h3 className="font-semibold text-gray-900 mb-4">创建新决策</h3>
                <div className="space-y-3">
                  <div>
                    <label className="text-xs font-medium text-gray-500 block mb-1">决策标题 *</label>
                    <input type="text" value={newDecision.title}
                      onChange={e => setNewDecision(p => ({...p, title: e.target.value}))}
                      className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary-500"
                      placeholder="例如：购买新冰箱" />
                  </div>
                  <div>
                    <label className="text-xs font-medium text-gray-500 block mb-1">背景说明</label>
                    <textarea value={newDecision.description}
                      onChange={e => setNewDecision(p => ({...p, description: e.target.value}))}
                      className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary-500"
                      rows={2} placeholder="可选" />
                  </div>
                  <div>
                    <label className="text-xs font-medium text-gray-500 block mb-2">备选方案</label>
                    {newDecision.options.map((opt, i) => (
                      <div key={i} className="mb-3 p-3 bg-gray-50 rounded-lg">
                        <div className="flex items-center gap-2 mb-2">
                          <input type="text" value={opt.name} placeholder={`方案 ${i+1} 名称`}
                            onChange={e => { const o = [...newDecision.options]; o[i].name = e.target.value; setNewDecision(p => ({...p, options: o})) }}
                            className="flex-1 px-3 py-1.5 border border-gray-300 rounded-lg text-sm focus:outline-none focus:ring-1 focus:ring-primary-500" />
                          {i > 0 && <button onClick={() => setNewDecision(p => ({...p, options: p.options.filter((_, j) => j !== i)}))}><Trash2 className="w-4 h-4 text-gray-400" /></button>}
                        </div>
                        <div className="grid grid-cols-2 gap-2">
                          <textarea value={opt.pros} placeholder={"优点（每行一条）"}
                            onChange={e => { const o = [...newDecision.options]; o[i].pros = e.target.value; setNewDecision(p => ({...p, options: o})) }}
                            className="px-2 py-1.5 border border-gray-200 rounded text-xs focus:outline-none focus:ring-1 focus:ring-green-400" rows={3} />
                          <textarea value={opt.cons} placeholder={"缺点（每行一条）"}
                            onChange={e => { const o = [...newDecision.options]; o[i].cons = e.target.value; setNewDecision(p => ({...p, options: o})) }}
                            className="px-2 py-1.5 border border-gray-200 rounded text-xs focus:outline-none focus:ring-1 focus:ring-red-400" rows={3} />
                        </div>
                      </div>
                    ))}
                    <button onClick={() => setNewDecision(p => ({...p, options: [...p.options, {name:'',pros:'',cons:''}]}))}
                      className="text-xs text-primary-600 hover:text-primary-700 flex items-center gap-1">
                      <Plus className="w-3.5 h-3.5" />添加方案
                    </button>
                  </div>
                  <div className="flex gap-2 pt-2">
                    <button onClick={submitDecision} disabled={!newDecision.title} className="btn-primary">提交分析</button>
                    <button onClick={() => setShowCreate(false)} className="btn-secondary">取消</button>
                  </div>
                </div>
              </div>
            )}

            {loading ? <div className="text-center py-12 text-gray-400">加载中...</div>
            : decisions.length === 0 ? (
              <div className="card text-center py-12">
                <Brain className="w-12 h-12 text-gray-300 mx-auto mb-3" />
                <p className="text-gray-500">暂无决策记录</p>
                <p className="text-xs text-gray-400 mt-1">创建一个决策，让 JiaBu 为你分析最优方案</p>
              </div>
            ) : decisions.map(d => (
              <div key={d.id} className="card">
                <div className="flex items-start justify-between gap-4">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <span className={`text-xs px-2 py-0.5 rounded-full ${
                        d.status === 'open' ? 'bg-blue-100 text-blue-700' :
                        d.status === 'decided' ? 'bg-green-100 text-green-700' :
                        'bg-gray-100 text-gray-500'
                      }`}>{d.status === 'open' ? '进行中' : d.status === 'decided' ? '已决策' : '已归档'}</span>
                      <h3 className="font-semibold text-gray-900 text-sm truncate">{d.title}</h3>
                    </div>
                    {d.description && <p className="text-xs text-gray-500 mb-2">{d.description}</p>}
                    <div className="flex flex-wrap gap-2">
                      {d.options?.map((opt: {name:string}) => (
                        <span key={opt.name} className={`text-xs px-2 py-0.5 rounded-full border ${
                          d.recommendation === opt.name ? 'border-primary-300 bg-primary-50 text-primary-700 font-medium' : 'border-gray-200 text-gray-500'
                        }`}>
                          {d.recommendation === opt.name && <Sparkles className="w-3 h-3 inline mr-0.5" />}
                          {opt.name}
                        </span>
                      ))}
                    </div>
                  </div>
                  <ChevronRight className="w-4 h-4 text-gray-300 shrink-0 mt-1" />
                </div>
                {d.recommendation && (
                  <div className="mt-3 p-2 bg-primary-50 rounded-lg flex items-center gap-2">
                    <AlertCircle className="w-3.5 h-3.5 text-primary-600 shrink-0" />
                    <span className="text-xs text-primary-700">推荐方案：<strong>{d.recommendation}</strong></span>
                  </div>
                )}
                <div className="mt-3 text-xs text-gray-400">
                  {new Date(d.created_at).toLocaleDateString('zh-CN')}
                </div>
              </div>
            ))}
          </div>
        )}
      </main>

      <footer className="text-center text-xs text-gray-400 py-6 border-t border-gray-200 mt-8">
        <p>JiaBu 决策助手 · <a href="https://hamr.store" className="hover:text-gray-600">HamR 家庭智能助理</a></p>
      </footer>
    </div>
  )
}
