import copy
import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('perf',Path(__file__).with_name('perf-games.py'))
perf=importlib.util.module_from_spec(spec); spec.loader.exec_module(perf)

class PerformanceContracts(unittest.TestCase):
    def fixture(self):
        samples=[{key:float(n) for key in (*perf.TIMINGS,*perf.WORK)} for n in range(1,101)]
        return {'schema':2,'machine':{'node':'test'},'config':{'frames':100},'games':{'demo':{
            'backend':'test','dimensions':[640,480],'metrics':perf.summarize(samples),'sizes':{'native_bytes':10000}}}}
    def test_nearest_rank_and_invalid_samples(self):
        row=self.fixture()['games']['demo']['metrics']['cpu_us']
        self.assertEqual((row['p50'],row['p95'],row['p99'],row['mean']),(50,95,99,50.5))
        with self.assertRaises(ValueError):perf.summarize([])
        with self.assertRaises(ValueError):perf.summarize([{'cpu_us':float('nan')}])
    def test_optional_async_gpu_and_quality_contracts(self):
        row=perf.summarize([{'cpu_us':1,'gpu_us':2},{'cpu_us':2,'gpu_us':None}])['gpu_us']
        self.assertEqual((row['samples'],row['coverage'],row['mean']),(1,.5,2))
        self.assertIsNone(perf.summarize([{'cpu_us':1}])['gpu_us'])
        with self.assertRaises(ValueError):perf.summarize([{'cpu_us':1},{'cpu_us':None}])
        old=self.fixture();current=copy.deepcopy(old)
        current['games']['demo']['metrics']['gpu_us']['coverage']=.4
        self.assertTrue(any('coverage' in x for x in perf.compare(current,old)))
        current=copy.deepcopy(old);current['games']['demo']['metrics']['gpu_us']['p99']+=100
        self.assertTrue(any('gpu_us/p99' in x for x in perf.compare(current,old)))
        current=copy.deepcopy(old);current['games']['demo']['render_quality']=2
        self.assertTrue(perf.compare(current,old))
    def test_regressions_fail_and_baseline_is_not_mutated(self):
        old=self.fixture(); current=copy.deepcopy(old)
        current['games']['demo']['metrics']['cpu_us']['p99']=999
        current['games']['demo']['sizes']['native_bytes']=20000
        failures=perf.compare(current,old)
        self.assertTrue(any('cpu_us/p99' in f for f in failures));self.assertTrue(any('native_bytes' in f for f in failures))
        self.assertEqual(old,self.fixture())
    def test_small_timer_noise_passes_but_work_growth_fails(self):
        old=self.fixture(); current=copy.deepcopy(old)
        current['games']['demo']['metrics']['cpu_us']['p99']+=50
        self.assertEqual(perf.compare(current,old),[])
        current['games']['demo']['metrics']['vertices']['max']=100000 # maxima are diagnostic; quantiles gate
        current['games']['demo']['metrics']['vertices']['mean']*=2
        self.assertTrue(perf.compare(current,old))
    def test_missing_coverage_and_incompatible_machine_fail_closed(self):
        old=self.fixture();current=copy.deepcopy(old);current['machine']['node']='other'
        self.assertTrue(perf.compare(current,old))
        current=copy.deepcopy(old);current['games']['demo']['metrics']['cpu_us']=None
        self.assertTrue(perf.compare(current,old))
        current=copy.deepcopy(old);current['games']={}
        self.assertTrue(perf.compare(current,old))

if __name__=='__main__':unittest.main()
