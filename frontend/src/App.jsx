import { apiFetch } from './api';
import React, { useState } from 'react';
import Layout from './components/Layout';
import MetricsPanel from './components/MetricsPanel';
import TaskForm from './components/TaskForm';
import DashboardTabs from './components/DashboardTabs';

function App() {
  const [searchResults, setSearchResults] = useState(() => {
    const saved = localStorage.getItem('searchResults');
    return saved ? JSON.parse(saved) : null;
  });

  // Task State
  const [tasks, setTasks] = useState(() => {
    const saved = localStorage.getItem('tasks');
    return saved ? JSON.parse(saved) : [];
  });

  React.useEffect(() => {
    // Sanitize tasks before saving to avoid QuotaExceededError
    // We strip out the 'results' field from each task
    const tasksToSave = tasks.map(task => {
      const { results, ...rest } = task;
      return rest;
    });
    try {
      localStorage.setItem('tasks', JSON.stringify(tasksToSave));
    } catch (e) {
      console.error("Failed to save tasks to localStorage:", e);
    }
  }, [tasks]);

  const addTask = (newTask) => {
    setTasks((prev) => [newTask, ...prev]);
  };

  const updateTask = (taskId, updates) => {
    setTasks((prev) => prev.map(task =>
      task.id === taskId ? { ...task, ...updates } : task
    ));
  };

  // We do NOT save searchResults to localStorage anymore to prevent quota errors
  // with large datasets.
  /*
  React.useEffect(() => {
    if (searchResults) {
      localStorage.setItem('searchResults', JSON.stringify(searchResults));
    } else {
      localStorage.removeItem('searchResults');
    }
  }, [searchResults]);
  */

  const clearSearchResults = () => {
    setSearchResults(null);
  };

  const handleViewTaskResults = (task) => {
    if (task.results) {
      setSearchResults(task.results);
    }
  };

  const deleteTask = async (taskId) => {
    // 1. Find the task to get its bucket info
    const taskToDelete = tasks.find(t => t.id === taskId);

    // 2. Optimistically remove from UI immediately
    setTasks((prev) => prev.filter(task => task.id !== taskId));

    if (taskToDelete && taskToDelete.results && taskToDelete.results.length > 0) {
      // Collect all bucket names associated with this task
      // Usually all results in one task go to the same bucket, but we check.
      const buckets = new Set();
      taskToDelete.results.forEach(item => {
        if (item.s3_bucket) {
          buckets.add(item.s3_bucket);
        }
      });

      if (buckets.size > 0) {
        try {
          const bucketList = Array.from(buckets);
          console.log("Deleting buckets:", bucketList);
          await apiFetch('/api/storage/delete', {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
            },
            body: JSON.stringify({
              bucket_names: bucketList
            }),
          });
        } catch (err) {
          console.error("Failed to delete remote task data:", err);
          // We don't restore the task in UI because the user wanted it gone. 
          // We just log the error.
        }
      }
    }
  };

  return (
    <Layout>
      <MetricsPanel />
      <div className="grid grid-cols-1 md:grid-cols-3 gap-8 mt-4">
        <TaskForm
          setSearchResults={setSearchResults}
          addTask={addTask}
          updateTask={updateTask}
        />
        <DashboardTabs
          searchResults={searchResults}
          clearSearchResults={clearSearchResults}
          tasks={tasks}
          onViewTaskResults={handleViewTaskResults}
          deleteTask={deleteTask}
        />
      </div>
    </Layout>
  );
}

export default App;
